use std::{
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
    injector::{self},
    media::download_configured_media,
    models::{DisplaySettings, MediaKind, RuntimeStatus},
    plugin::hello_result,
    protocol::parse_configure,
};

const MAX_INLINE_MEDIA: usize = 16 * 1024 * 1024;

struct Session {
    revision: String,
    payload_revision: String,
    kind: MediaKind,
    display: DisplaySettings,
    media_url: String,
}

struct Runtime {
    status: RuntimeStatus,
    port: Option<u16>,
    saw_absent: bool,
    next_takeover: Option<Instant>,
}

pub struct WorkerState {
    shutting_down: AtomicBool,
    paused: AtomicBool,
    session: Mutex<Option<Session>>,
    runtime: Mutex<Runtime>,
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
            runtime: Mutex::new(Runtime {
                status: RuntimeStatus::default(),
                port: None,
                saw_absent: !running,
                next_takeover: None,
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
        {
            let mut session = self.session.lock().map_err(|_| "锁已损坏。".to_string())?;
            *session = Some(Session {
                revision: spec.revision.clone(),
                payload_revision,
                kind: spec.media.kind.clone(),
                display: spec.display,
                media_url,
            });
        }
        self.set_message("waiting", "背景已配置，等待 Cursor Agent 窗口");
        let _ = self.sync_injection(false);
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
        self.paused.store(true, Ordering::SeqCst);
        if let Some(port) = self.current_port() {
            let _ = injector::clear_agent_pages(port);
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
        let _ = self.sync_injection(false);
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
        let session = self
            .session
            .lock()
            .map_err(|_| "锁已损坏。".to_string())?
            .clone();
        let Some(session) = session else {
            return Ok(());
        };
        let count = injector::inject_media(
            port,
            &session.media_url,
            &session.kind,
            &session.display,
            &session.payload_revision,
        )?;
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.port = Some(port);
            runtime.status.active_targets = count;
            runtime.status.phase = "active".to_string();
            runtime.status.message = if count == 0 {
                "调试口已连接，等待 Agent 窗口".to_string()
            } else {
                format!("背景已应用到 {count} 个 Agent 窗口")
            };
            runtime.status.last_error = None;
        }
        Ok(())
    }
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            revision: self.revision.clone(),
            payload_revision: self.payload_revision.clone(),
            kind: self.kind.clone(),
            display: self.display.clone(),
            media_url: self.media_url.clone(),
        }
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
