use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    cursor_host::{self, browser_processes},
    injector::{self, Injection},
    media::download_configured_media,
    models::RuntimeStatus,
    payload::install_script,
    plugin::hello_result,
    protocol::{parse_configure, ConfigureSpec},
};

const MAX_INLINE_MEDIA: usize = 16 * 1024 * 1024;

#[derive(Clone)]
struct Session {
    revision: String,
    default: Injection,
    independent: bool,
    windows: HashMap<String, Injection>,
}

fn update_session(
    session: &mut Option<Session>,
    spec: &ConfigureSpec,
    injection: Injection,
) -> Result<(), String> {
    if let Some(target_id) = &spec.target_id {
        let current = session
            .as_mut()
            .filter(|s| s.independent)
            .ok_or_else(|| "请先配置独立窗口模式。".to_string())?;
        current.windows.insert(target_id.clone(), injection);
    } else {
        let windows = session
            .as_mut()
            .filter(|s| s.independent && spec.independent_windows)
            .map(|s| std::mem::take(&mut s.windows))
            .unwrap_or_default();
        *session = Some(Session {
            revision: spec.revision.clone(),
            default: injection,
            independent: spec.independent_windows,
            windows,
        });
    }
    Ok(())
}

struct Runtime {
    status: RuntimeStatus,
    port: Option<u16>,
    saw_absent: bool,
    next_takeover: Option<Instant>,
    target_ids: Vec<String>,
}

pub struct WorkerState {
    shutting_down: AtomicBool,
    paused: AtomicBool,
    session: Mutex<Option<Session>>,
    runtime: Mutex<Runtime>,
    injection_lock: Mutex<()>,
}

impl WorkerState {
    pub fn load() -> Result<Self, String> {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
        Self::load_from(PathBuf::from(base).join("CursorAgentBackgroundStudio"))
    }

    pub fn load_from(dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let processes = cursor_host::list_processes().unwrap_or_default();
        let running = !browser_processes(&processes).is_empty();
        Ok(Self {
            shutting_down: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            session: Mutex::new(None),
            injection_lock: Mutex::new(()),
            runtime: Mutex::new(Runtime {
                status: RuntimeStatus::default(),
                port: None,
                saw_absent: !running,
                next_takeover: None,
                target_ids: Vec::new(),
            }),
        })
    }

    pub fn hello() -> Value {
        hello_result()
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub async fn configure(&self, params: Value) -> Result<Value, String> {
        let spec = parse_configure(&params)?;
        if spec.media.byte_size > MAX_INLINE_MEDIA as u64 {
            return Err("暂只支持 16 MB 以内的背景。".to_string());
        }
        let bytes = download_configured_media(&spec).await?;
        if bytes.len() > MAX_INLINE_MEDIA {
            return Err("0.1.0 暂只支持 16 MB 以内的背景。".to_string());
        }
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = format!("{:x}", hasher.finalize());
        let payload_revision = format!("{}:{}", spec.revision, &digest[..12]);
        let media_url = format!(
            "data:{};base64,{}",
            spec.media.mime_type,
            STANDARD.encode(&bytes)
        );
        let script = install_script(
            &media_url,
            &spec.media.kind,
            &spec.display,
            &payload_revision,
        )?;
        {
            let _guard = self
                .injection_lock
                .lock()
                .map_err(|_| "锁已损坏。".to_string())?;
            let mut session = self.session.lock().map_err(|_| "锁已损坏。".to_string())?;
            let injection = Injection {
                revision: payload_revision,
                script: Arc::from(script),
            };
            update_session(&mut session, &spec, injection)?;
        }
        if self.paused.load(Ordering::SeqCst) {
            return self.status_value();
        }
        self.set_message("waiting", "背景已配置，等待 Cursor Agent 窗口");
        if let Err(error) = self.sync_injection(false) {
            self.set_message("error", &error);
        }
        self.status_value()
    }

    pub fn status_value(&self) -> Result<Value, String> {
        let runtime = self.runtime.lock().map_err(|_| "锁已损坏。".to_string())?;
        let session = self.session.lock().map_err(|_| "锁已损坏。".to_string())?;
        Ok(json!({
            "pluginProtocol": 2,
            "pluginId": "cursor-agent",
            "version": env!("CARGO_PKG_VERSION"),
            "phase": runtime.status.phase,
            "message": runtime.status.message,
            "activeTargets": runtime.status.active_targets,
            "paused": self.paused.load(Ordering::SeqCst),
            "configured": session.is_some(),
            "revision": session.as_ref().map(|item| item.revision.clone()),
            "independentWindows": session.as_ref().is_some_and(|item| item.independent),
            "targetIds": runtime.target_ids,
            "configuredTargetIds": session.as_ref().map(|item| item.windows.keys().collect::<Vec<_>>()).unwrap_or_default(),
            "lastError": runtime.status.last_error,
        }))
    }

    pub fn apply_blocking(&self) -> Result<Value, String> {
        self.paused.store(false, Ordering::SeqCst);
        {
            let mut runtime = self.runtime.lock().map_err(|_| "锁已损坏。".to_string())?;
            runtime.saw_absent = true;
            runtime.next_takeover = None;
        }
        self.sync_injection(true)?;
        self.status_value()
    }

    pub fn pause_blocking(&self) -> Result<Value, String> {
        let _guard = self
            .injection_lock
            .lock()
            .map_err(|_| "锁已损坏。".to_string())?;
        self.paused.store(true, Ordering::SeqCst);
        if let Some(port) = self.current_port() {
            injector::clear_agent_pages(port)?;
        }
        self.set_message("paused", "已暂停，Agent 窗口恢复为官方外观");
        self.status_value()
    }

    pub fn restore_blocking(&self) -> Result<Value, String> {
        self.pause_blocking()
    }

    pub fn shutdown(&self) -> Result<Value, String> {
        self.shutting_down.store(true, Ordering::SeqCst);
        Ok(json!({ "shutdown": true, "keptTarget": true }))
    }

    pub fn tick(&self) {
        if self.is_shutting_down() || self.paused.load(Ordering::SeqCst) {
            return;
        }
        if self
            .session
            .lock()
            .ok()
            .and_then(|session| session.as_ref().map(|_| ()))
            .is_none()
        {
            return;
        }
        if let Err(error) = self.sync_injection(false) {
            self.set_message("error", &error);
        }
    }

    fn current_port(&self) -> Option<u16> {
        self.runtime.lock().ok().and_then(|runtime| runtime.port)
    }

    fn set_message(&self, phase: &str, message: &str) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.status.phase = phase.to_string();
            runtime.status.message = message.to_string();
            runtime.status.last_error = None;
        }
    }

    fn sync_injection(&self, allow_restart: bool) -> Result<(), String> {
        let processes = cursor_host::list_processes()?;
        let browsers = browser_processes(&processes);
        if let Some(port) = cursor_host::debug_port(&processes) {
            self.inject_port(port)?;
            return Ok(());
        }
        if browsers.is_empty() {
            if let Ok(mut runtime) = self.runtime.lock() {
                runtime.saw_absent = true;
                runtime.port = None;
                runtime.status.phase = "waiting".to_string();
                runtime.status.message = "等待 Cursor 启动".to_string();
                runtime.status.active_targets = 0;
                runtime.target_ids.clear();
            }
            if let Ok(mut session) = self.session.lock() {
                if let Some(session) = session.as_mut() {
                    session.windows.clear();
                }
            }
            return Ok(());
        }
        let saw_absent = self
            .runtime
            .lock()
            .map(|runtime| runtime.saw_absent)
            .unwrap_or(false);
        if !allow_restart && !saw_absent {
            self.set_message(
                "blocked",
                "Cursor 已在运行。点应用后才会重启，并用原来的登录配置打开调试口。",
            );
            return Ok(());
        }
        if !allow_restart {
            let wait = self
                .runtime
                .lock()
                .ok()
                .and_then(|runtime| runtime.next_takeover)
                .is_some_and(|when| Instant::now() < when);
            if wait {
                return Ok(());
            }
        }
        self.relaunch(&processes)
    }

    fn relaunch(&self, processes: &[cursor_host::CursorProcess]) -> Result<(), String> {
        let browsers = browser_processes(processes);
        let executable = cursor_host::executable(processes)?;
        let user_data = cursor_host::user_data_dir(processes);
        self.set_message("starting", "正在用原来的登录配置重启 Cursor，并打开调试口");
        if let Err(error) = cursor_host::request_close(&browsers) {
            if let Ok(mut runtime) = self.runtime.lock() {
                runtime.next_takeover = Some(Instant::now() + Duration::from_secs(15));
                runtime.status.phase = "blocked".to_string();
                runtime.status.message = error.clone();
                runtime.status.last_error = Some(error);
            }
            return Ok(());
        }
        let port = cursor_host::select_port(9338)?;
        if let Err(error) = cursor_host::launch(&executable, &user_data, port) {
            self.set_message("error", &error);
            return Err(error);
        }
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.saw_absent = false;
        }
        self.inject_port(port)
    }

    fn inject_port(&self, port: u16) -> Result<(), String> {
        let _guard = self
            .injection_lock
            .lock()
            .map_err(|_| "锁已损坏。".to_string())?;
        if self.paused.load(Ordering::SeqCst) {
            return Ok(());
        }
        let session = self
            .session
            .lock()
            .map_err(|_| "锁已损坏。".to_string())?
            .clone();
        let Some(session) = session else {
            return Ok(());
        };
        let report = injector::sync_scripts(
            port,
            (!session.independent).then_some(&session.default),
            &session.windows,
            false,
        )?;
        let count = report.applied;
        if let Ok(mut current) = self.session.lock() {
            if let Some(current) = current.as_mut() {
                current.windows.retain(|id, _| report.present.contains(id));
            }
        }
        if let Ok(mut runtime) = self.runtime.lock() {
            let mut targets = report.targets;
            // Keep known Agent identities during transient navigation failures.
            for id in &runtime.target_ids {
                if report.failed.contains(id) && !targets.contains(id) {
                    targets.push(id.clone());
                }
            }
            targets.sort();
            runtime.target_ids = targets;
            runtime.port = Some(port);
            runtime.status.active_targets = count;
            runtime.status.phase = "active".to_string();
            runtime.status.message =
                if count == 0 && session.independent && !runtime.target_ids.is_empty() {
                    format!(
                        "已发现 {} 个 Agent 窗口，等待宿主分配独立背景",
                        runtime.target_ids.len()
                    )
                } else if count == 0 {
                    "调试口已连接，等待 Agent 窗口".to_string()
                } else {
                    format!("背景已应用到 {count} 个 Agent 窗口")
                };
            runtime.status.last_error =
                (!report.errors.is_empty()).then(|| report.errors.join("；"));
        }
        Ok(())
    }
}

pub fn start_tick(state: Arc<WorkerState>) {
    tokio::spawn(async move {
        loop {
            if state.is_shutting_down() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            let worker = Arc::clone(&state);
            let _ = tokio::task::spawn_blocking(move || worker.tick()).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targeted_updates_are_atomic_and_base_updates_preserve_other_windows() {
        let mut spec = parse_configure(&json!({"schemaVersion": 1, "revision": "base", "independentWindows": true,
            "media": {"url": "http://127.0.0.1:9/x", "kind": "image", "mimeType": "image/png", "sha256": "a".repeat(64), "byteSize": 1}, "display": {}})).unwrap();
        let injection = |s: &str| Injection {
            revision: s.to_string(),
            script: Arc::from(s),
        };
        let mut session = None;
        spec.target_id = Some("A".to_string());
        assert!(update_session(&mut session, &spec, injection("a")).is_err());
        assert!(session.is_none());
        spec.target_id = None;
        update_session(&mut session, &spec, injection("base")).unwrap();
        for id in ["A", "B"] {
            spec.target_id = Some(id.to_string());
            update_session(&mut session, &spec, injection(id)).unwrap();
        }
        let before_b = session.as_ref().unwrap().windows["B"].script.clone();
        spec.target_id = Some("A".into());
        update_session(&mut session, &spec, injection("new-A")).unwrap();
        assert!(Arc::ptr_eq(
            &before_b,
            &session.as_ref().unwrap().windows["B"].script
        ));
        spec.target_id = None;
        update_session(&mut session, &spec, injection("new-base")).unwrap();
        assert_eq!(session.as_ref().unwrap().windows.len(), 2);
        assert_eq!(&*session.as_ref().unwrap().windows["A"].script, "new-A");
        spec.independent_windows = false;
        update_session(&mut session, &spec, injection("legacy")).unwrap();
        assert!(session.as_ref().unwrap().windows.is_empty());
    }
}
