use std::time::Duration;

use serde_json::{json, Value};
use tungstenite::{connect, Message};

use crate::models::{DisplaySettings, MediaKind};
use crate::payload::{install_script, AGENT_PROBE, CLEANUP_SCRIPT};

pub fn inject_media(
    port: u16,
    media_url: &str,
    kind: &MediaKind,
    display: &DisplaySettings,
    revision: &str,
) -> Result<u32, String> {
    let script = install_script(media_url, kind, display, revision)?;
    apply_to_agent_pages(port, &script)
}

pub fn clear_agent_pages(port: u16) -> Result<u32, String> {
    apply_to_agent_pages(port, CLEANUP_SCRIPT)
}

fn apply_to_agent_pages(port: u16, script: &str) -> Result<u32, String> {
    let mut applied = 0;
    for page in list_pages(port)? {
        let url = page
            .get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .unwrap_or("");
        let url = normalize_ws(url, port)?;
        let mut socket = connect_page(&url)?;
        let probe = evaluate(&mut socket, AGENT_PROBE)?;
        if probe.as_bool() != Some(true) {
            let _ = socket.close(None);
            continue;
        }
        let result = evaluate(&mut socket, script)?;
        let _ = socket.close(None);
        if result.get("installed").and_then(Value::as_bool) == Some(true)
            || result.as_bool() == Some(true)
        {
            applied += 1;
        }
    }
    Ok(applied)
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
