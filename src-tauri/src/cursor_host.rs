use std::{
    net::TcpListener,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

#[derive(Clone, Debug)]
pub struct CursorProcess {
    pub pid: u32,
    pub executable: String,
    pub command_line: String,
}

pub fn list_processes() -> Result<Vec<CursorProcess>, String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name = 'Cursor.exe'\" | Select-Object ProcessId,ExecutablePath,CommandLine | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| format!("读取 Cursor 进程失败：{error}"))?;
    if !output.status.success() {
        return Err("读取 Cursor 进程失败。".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("解析进程列表失败：{error}"))?;
    let items = match value {
        Value::Array(items) => items,
        other => vec![other],
    };
    let mut processes = Vec::new();
    for item in items {
        let Some(pid) = item.get("ProcessId").and_then(Value::as_u64) else {
            continue;
        };
        processes.push(CursorProcess {
            pid: pid as u32,
            executable: item
                .get("ExecutablePath")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            command_line: item
                .get("CommandLine")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(processes)
}

pub fn browser_processes(processes: &[CursorProcess]) -> Vec<CursorProcess> {
    processes
        .iter()
        .filter(|process| is_browser(process))
        .cloned()
        .collect()
}

fn is_browser(process: &CursorProcess) -> bool {
    let line = &process.command_line;
    !line.contains("--type=")
        && !line.contains("resources\\")
        && !line.contains("extensions\\")
        && !line.contains("serverWorkerMain")
        && !line.contains("jsonServerMain")
        && !line.contains("gitWorker")
        && !line.contains("cursor-glass-probe")
}

pub fn debug_port(processes: &[CursorProcess]) -> Option<u16> {
    processes
        .iter()
        .find_map(|process| parse_port(&process.command_line))
}

fn parse_port(command_line: &str) -> Option<u16> {
    let marker = "--remote-debugging-port=";
    let start = command_line.find(marker)? + marker.len();
    let digits: String = command_line[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    let port = digits.parse::<u16>().ok()?;
    if port == 0 {
        None
    } else {
        Some(port)
    }
}

pub fn executable(processes: &[CursorProcess]) -> Result<PathBuf, String> {
    if let Some(path) = processes.iter().find_map(|process| {
        let path = PathBuf::from(&process.executable);
        path.is_file().then_some(path)
    }) {
        return Ok(path);
    }
    for candidate in candidates() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("未找到 Cursor.exe。".to_string())
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        paths.push(PathBuf::from(local).join(r"Programs\cursor\Cursor.exe"));
    }
    paths.push(PathBuf::from(r"D:\cursor system\cursor\Cursor.exe"));
    paths
}

pub fn user_data_dir(processes: &[CursorProcess]) -> PathBuf {
    for process in processes {
        if let Some(dir) = user_data_from_line(&process.command_line) {
            return PathBuf::from(dir);
        }
    }
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(appdata).join("Cursor")
}

fn user_data_from_line(command_line: &str) -> Option<String> {
    let marker = "--user-data-dir=";
    let start = command_line.find(marker)? + marker.len();
    let rest = command_line[start..].trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        return stripped.split('"').next().map(str::to_string);
    }
    Some(
        rest.split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('"')
            .to_string(),
    )
}

pub fn select_port(preferred: u16) -> Result<u16, String> {
    for port in std::iter::once(preferred).chain(9340..9360) {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err("没有可用的本机调试端口。".to_string())
}

pub fn request_close(processes: &[CursorProcess]) -> Result<(), String> {
    if processes.is_empty() {
        return Ok(());
    }
    for process in processes {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &process.pid.to_string()])
            .status();
    }
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        let alive = list_processes()?;
        let still = browser_processes(&alive)
            .iter()
            .any(|live| processes.iter().any(|old| old.pid == live.pid));
        if !still {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(400));
    }
    Err("Cursor 还在运行。请在保存确认框上点同意后再应用。".to_string())
}

pub fn launch(executable: &PathBuf, user_data: &PathBuf, port: u16) -> Result<(), String> {
    Command::new(executable)
        .arg(format!("--user-data-dir={}", user_data.display()))
        .arg("--remote-debugging-address=127.0.0.1")
        .arg(format!("--remote-debugging-port={port}"))
        .spawn()
        .map_err(|error| format!("启动 Cursor 失败：{error}"))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if crate::injector::debug_port_ready(port) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(400));
    }
    Err("Cursor 已启动，但调试口没有在 60 秒内打开。".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_debug_port_and_ignores_helper_processes() {
        assert_eq!(
            parse_port(r#"Cursor.exe --remote-debugging-port=9338"#),
            Some(9338)
        );
        let helper = CursorProcess {
            pid: 2,
            executable: String::new(),
            command_line: r#"Cursor.exe --type=renderer --user-data-dir=C:\Cursor"#.to_string(),
        };
        let browser = CursorProcess {
            pid: 1,
            executable: String::new(),
            command_line: r#""D:\cursor system\cursor\Cursor.exe" --user-data-dir=C:\Users\ZhuanZ1\AppData\Roaming\Cursor --remote-debugging-port=9338"#.to_string(),
        };
        assert!(browser_processes(&[helper, browser.clone()]).len() == 1);
        assert_eq!(user_data_dir(&[browser]).ends_with("Cursor"), true);
    }
}
