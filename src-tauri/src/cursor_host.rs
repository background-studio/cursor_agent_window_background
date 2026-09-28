use std::{
    net::TcpListener,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
pub struct CursorProcess {
    pub pid: u32,
    pub executable: String,
    pub command_line: String,
}

pub fn list_processes() -> Result<Vec<CursorProcess>, String> {
    let mut processes = Vec::new();
    win::for_each_process(|pid, name| {
        if !name.eq_ignore_ascii_case("Cursor.exe") {
            return;
        }
        let Some(handle) = win::open_process(pid) else {
            return;
        };
        processes.push(CursorProcess {
            pid,
            executable: win::image_path(&handle).unwrap_or_default(),
            command_line: win::command_line(&handle).unwrap_or_default(),
        });
    })?;
    Ok(processes)
}

mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn NtQueryInformationProcess(
            process_handle: HANDLE,
            information_class: u32,
            process_information: *mut core::ffi::c_void,
            process_information_length: u32,
            return_length: *mut u32,
        ) -> i32;
    }

    pub struct HandleGuard(HANDLE);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    fn utf16_to_string(buffer: &[u16]) -> String {
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    pub fn open_process(pid: u32) -> Option<HandleGuard> {
        if pid == 0 {
            return None;
        }
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return None;
        }
        Some(HandleGuard(handle))
    }

    pub fn image_path(process: &HandleGuard) -> Option<String> {
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok =
            unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut size) };
        if ok == 0 || size == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }

    pub fn command_line(process: &HandleGuard) -> Option<String> {
        let mut needed = 0u32;
        unsafe {
            NtQueryInformationProcess(
                process.0,
                PROCESS_COMMAND_LINE_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
        }
        let size = if needed == 0 { 8192 } else { needed.max(16) };
        let mut buffer = vec![0u8; size as usize];
        let mut returned = 0u32;
        let status = unsafe {
            NtQueryInformationProcess(
                process.0,
                PROCESS_COMMAND_LINE_INFORMATION,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut returned,
            )
        };
        if status != 0 || buffer.len() < std::mem::size_of::<UnicodeString>() {
            return None;
        }
        let header = unsafe { &*(buffer.as_ptr() as *const UnicodeString) };
        if header.buffer.is_null() || header.length == 0 {
            return None;
        }
        let units = (header.length as usize) / 2;
        let slice = unsafe { std::slice::from_raw_parts(header.buffer, units) };
        Some(String::from_utf16_lossy(slice))
    }

    pub fn for_each_process(mut visit: impl FnMut(u32, &str)) -> Result<(), String> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return Err("无法创建进程快照。".to_string());
        }
        let _guard = HandleGuard(snapshot);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            cntUsage: 0,
            th32ProcessID: 0,
            th32DefaultHeapID: 0,
            th32ModuleID: 0,
            cntThreads: 0,
            th32ParentProcessID: 0,
            pcPriClassBase: 0,
            dwFlags: 0,
            szExeFile: [0; 260],
        };
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
        while ok != 0 {
            visit(entry.th32ProcessID, &utf16_to_string(&entry.szExeFile));
            ok = unsafe { Process32NextW(snapshot, &mut entry) };
        }
        Ok(())
    }
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
            .creation_flags(CREATE_NO_WINDOW)
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
    fn lists_processes_in_process_without_helpers() {
        assert!(list_processes().is_ok());
    }

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
