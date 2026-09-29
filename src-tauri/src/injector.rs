use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::{json, Value};
use tungstenite::{connect, Message};

use crate::{
    payload::{AGENT_PROBE, CLEANUP_SCRIPT},
    protocol::valid_target_id,
};

#[derive(Clone)]
pub struct Injection {
    pub revision: String,
    pub script: Arc<str>,
}

#[derive(Default)]
pub struct InjectionReport {
    pub applied: u32,
    pub targets: Vec<String>,
    pub present: Vec<String>,
    pub failed: Vec<String>,
    pub errors: Vec<String>,
}

pub fn sync_scripts(
    port: u16,
    default: Option<&Injection>,
    windows: &HashMap<String, Injection>,
    cleanup: bool,
) -> Result<InjectionReport, String> {
    let mut report = InjectionReport::default();
    for page in list_pages(port)? {
        let Some(id) = page
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_target_id(id))
        else {
            continue;
        };
        report.present.push(id.to_string());
        let url = page
            .get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .unwrap_or("");
        // A closing or navigating page must not prevent other windows updating.
        let result = (|| -> Result<(), String> {
            let url = normalize_ws(url, port)?;
            let mut socket = connect_page(&url)?;
            let probe = evaluate(&mut socket, AGENT_PROBE)?;
            if probe.as_bool() != Some(true) {
                // If a previously injected page becomes an editor/loading page,
                // restore only our own state; never leave Agent-only CSS behind.
                evaluate(&mut socket, CLEANUP_SCRIPT)?;
                let _ = socket.close(None);
                return Ok(());
            }
            report.targets.push(id.to_string());
            let injection = windows.get(id).or(default);
            let script = if cleanup {
                CLEANUP_SCRIPT
            } else if let Some(injection) = injection {
                let revision = serde_json::to_string(&injection.revision)
                    .map_err(|error| error.to_string())?;
                let check = format!("window.__CURSOR_AGENT_BACKGROUND_STUDIO__?.revision==={revision}&&window.__CURSOR_AGENT_BACKGROUND_STUDIO__?.isHealthy?.()===true");
                if evaluate(&mut socket, &check)?.as_bool() == Some(true) {
                    report.applied += 1;
                    let _ = socket.close(None);
                    return Ok(());
                }
                &injection.script
            } else {
                let _ = socket.close(None);
                return Ok(());
            };
            let result = evaluate(&mut socket, script)?;
            let _ = socket.close(None);
            if result.get("installed").and_then(Value::as_bool) == Some(true)
                || result.as_bool() == Some(true)
            {
                report.applied += 1;
            }
            Ok(())
        })();
        if let Err(error) = result {
            report.failed.push(id.to_string());
            report.errors.push(format!("窗口 {id}：{error}"));
        }
    }
    Ok(report)
}

pub fn clear_agent_pages(port: u16) -> Result<u32, String> {
    let report = sync_scripts(port, None, &HashMap::new(), true)?;
    if !report.errors.is_empty() {
        return Err(report.errors.join("；"));
    }
    Ok(report.applied)
}

pub fn debug_port_ready(port: u16) -> bool {
    version(port).is_ok()
}

fn version(port: u16) -> Result<Value, String> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| error.to_string())?;
    let body = client
        .get(format!("http://127.0.0.1:{port}/json/version"))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .text()
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&body).map_err(|error| error.to_string())
}

fn list_pages(port: u16) -> Result<Vec<Value>, String> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| error.to_string())?;
    let body = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()
        .map_err(|error| format!("读取调试页面失败：{error}"))?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .text()
        .map_err(|error| error.to_string())?;
    let pages: Vec<Value> = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    Ok(pages
        .into_iter()
        .filter(|page| page.get("type").and_then(Value::as_str) == Some("page"))
        .collect())
}

fn normalize_ws(url: &str, port: u16) -> Result<String, String> {
    let rewritten = url.replacen("ws://localhost:", &format!("ws://127.0.0.1:"), 1);
    let expected = format!("ws://127.0.0.1:{port}/");
    if !rewritten.starts_with(&expected) {
        return Err("拒绝非本机调试地址。".to_string());
    }
    Ok(rewritten)
}

fn connect_page(
    url: &str,
) -> Result<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>, String>
{
    let (mut socket, _) = connect(url).map_err(|error| format!("连接调试页失败：{error}"))?;
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .map_err(|error| error.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(20)))
            .map_err(|error| error.to_string())?;
    }
    Ok(socket)
}

fn evaluate(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    expression: &str,
) -> Result<Value, String> {
    let result = call(
        socket,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true
        }),
    )?;
    if result.get("exceptionDetails").is_some() {
        return Err("页面脚本执行失败。".to_string());
    }
    Ok(result
        .get("result")
        .and_then(|value| value.get("value"))
        .cloned()
        .unwrap_or(Value::Null))
}

fn call(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let id = 1;
    socket
        .send(Message::Text(
            json!({"id": id, "method": method, "params": params})
                .to_string()
                .into(),
        ))
        .map_err(|error| error.to_string())?;
    loop {
        let message = socket.read().map_err(|error| error.to_string())?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if value.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("调试命令失败：{error}"));
        }
        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::Mutex,
        thread,
    };

    #[test]
    fn routes_by_target_recovers_layers_and_isolates_broken_pages_and_ide() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let installed = Arc::new(Mutex::new(HashMap::from([(
            "ide".to_string(),
            "stale-agent-layer".to_string(),
        )])));
        let writes = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let server_installed = installed.clone();
        let server_writes = writes.clone();
        let server = thread::spawn(move || {
            for _ in 0..4 {
                let (mut http, _) = listener.accept().unwrap();
                http.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut reader = BufReader::new(&mut http);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                }
                let ids = ["broken", "ide", "A", "B", "C"];
                let pages = ids.iter().map(|id| json!({"id": id, "type": "page", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/{id}")})).collect::<Vec<_>>();
                let body = serde_json::to_string(&pages).unwrap();
                write!(
                    http,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                drop(http);
                for id in ids {
                    let (stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut ws = tungstenite::accept(stream).unwrap();
                    while let Ok(Message::Text(text)) = ws.read() {
                        let request: Value = serde_json::from_str(&text).unwrap();
                        let expression = request["params"]["expression"].as_str().unwrap();
                        if id == "broken" {
                            ws.send(Message::Text(
                                json!({"id": 1, "error": {"message": "navigating"}})
                                    .to_string()
                                    .into(),
                            ))
                            .unwrap();
                            break;
                        }
                        let value = if expression == AGENT_PROBE {
                            json!(id != "ide")
                        } else if expression
                            .starts_with("window.__CURSOR_AGENT_BACKGROUND_STUDIO__?.revision===")
                        {
                            json!(server_installed.lock().unwrap().get(id).is_some_and(|rev| {
                                expression.contains(&format!(
                                    "==={}",
                                    serde_json::to_string(rev).unwrap()
                                ))
                            }))
                        } else if expression == CLEANUP_SCRIPT {
                            server_installed.lock().unwrap().remove(id);
                            json!(true)
                        } else {
                            server_writes
                                .lock()
                                .unwrap()
                                .push((id.to_string(), expression.to_string()));
                            server_installed
                                .lock()
                                .unwrap()
                                .insert(id.to_string(), expression.to_string());
                            json!({"installed": true})
                        };
                        ws.send(Message::Text(
                            json!({"id": 1, "result": {"result": {"value": value}}})
                                .to_string()
                                .into(),
                        ))
                        .unwrap();
                    }
                }
            }
        });
        let injection = |value: &str| Injection {
            revision: value.to_string(),
            script: Arc::from(value),
        };
        let mut windows = HashMap::from([
            ("A".to_string(), injection("image-A")),
            ("B".to_string(), injection("image-B")),
        ]);
        let first = sync_scripts(port, None, &windows, false).unwrap();
        assert_eq!(first.applied, 2);
        assert_eq!(first.targets, ["A", "B", "C"]);
        assert_eq!(first.failed, ["broken"]);
        assert_eq!(writes.lock().unwrap().len(), 2);
        assert!(!installed.lock().unwrap().contains_key("ide"));
        windows.insert("C".to_string(), injection("image-C"));
        assert_eq!(
            sync_scripts(port, None, &windows, false).unwrap().applied,
            3
        );
        assert_eq!(
            writes.lock().unwrap().len(),
            3,
            "new window must not reinstall A/B"
        );
        installed.lock().unwrap().remove("A");
        assert_eq!(
            sync_scripts(port, None, &windows, false).unwrap().applied,
            3
        );
        assert_eq!(
            writes.lock().unwrap().len(),
            4,
            "missing A layer must be reinstalled without touching B/C"
        );
        assert_eq!(
            sync_scripts(port, None, &HashMap::new(), true)
                .unwrap()
                .applied,
            3
        );
        server.join().unwrap();
        assert!(installed.lock().unwrap().is_empty());
        assert!(!writes.lock().unwrap().iter().any(|(id, _)| id == "ide"));
    }
}
