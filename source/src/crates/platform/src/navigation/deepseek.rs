//! Optional DSH desktop plugin. A successful response proves that the renderer
//! selected the requested session; merely waking the application is not enough.
use agent_hud_core::l10n;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Read, path::Path, time::Duration};

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint { version: u8, port: u16, token: String }
fn endpoint(bytes: &[u8]) -> Result<Endpoint, String> {
    let value: Endpoint = serde_json::from_slice(bytes).map_err(|_| unavailable())?;
    if bytes.len() > 2048 || value.version != 1 || value.port == 0 || value.token.len() != 64
        || !value.token.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(unavailable()); }
    Ok(value)
}
fn unavailable() -> String {
    l10n::text("DSH 会话跳转插件尚未就绪，请打开或重启 DSH 后重试", "DSH navigation plugin is not ready; open or restart DSH and retry")
}
fn navigate(client: &Client, endpoint: &Endpoint, id: &str) -> Result<bool, String> {
    if !valid_id(id) { return Err("Invalid DSH session ID".into()); }
    let request = format!("hud-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let response = client.post(format!("http://127.0.0.1:{}/open", endpoint.port))
        .bearer_auth(&endpoint.token).json(&json!({"requestId":request,"sessionId":id}))
        .send().map_err(|_| unavailable())?;
    if !response.status().is_success() { return Err(unavailable()); }
    let mut bytes = Vec::new();
    response.take(2049).read_to_end(&mut bytes).map_err(|_| unavailable())?;
    if bytes.len() > 2048 { return Err(unavailable()); }
    let receipt: Value = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if receipt["ok"] != true || receipt["sessionId"] != id || receipt["requestId"] != request { return Err(unavailable()); }
    Ok(true)
}
pub(crate) fn open(home: &Path, id: &str) -> Result<bool, String> {
    if !valid_id(id) { return Err("Invalid DSH session ID".into()); }
    let directory = match std::env::var("DSH_HOME").ok().filter(|p| !p.trim().is_empty()) {
        Some(path) if path == "~" => home.to_path_buf(),
        Some(path) if path.starts_with("~/") || path.starts_with("~\\") => home.join(&path[2..]),
        Some(path) => path.into(),
        None => home.join(".dsh"),
    };
    let mut bytes = Vec::new();
    std::fs::File::open(directory.join("agent-hub-navigation.json"))
        .map_err(|_| unavailable())?.take(2049).read_to_end(&mut bytes).map_err(|_| unavailable())?;
    let endpoint = endpoint(&bytes)?;
    let client = Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8)).build().map_err(|_| unavailable())?;
    // This fixed scheme only raises the desktop. The plugin performs and
    // acknowledges the exact navigation separately.
    crate::os::open_session_url("dsh://open")?;
    // Hidden Electron renderers may throttle their heartbeat. Raising the app
    // precedes a bounded readiness wait; never replay an /open request.
    let ready_client = Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(500)).build().map_err(|_| unavailable())?;
    let deadline=std::time::Instant::now()+Duration::from_secs(3);
    loop {
        let ready=ready_client.get(format!("http://127.0.0.1:{}/status",endpoint.port))
            .bearer_auth(&endpoint.token).send().ok().filter(|r|r.status().is_success())
            .and_then(|r|r.json::<Value>().ok()).is_some_and(|v|v["version"]==1&&v["clientReady"]==true);
        if ready {break;}
        if std::time::Instant::now()>=deadline {return Err(unavailable());}
        std::thread::sleep(Duration::from_millis(100));
    }
    navigate(&client, &endpoint, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deepseek_destination_rejects_paths_commands_and_bad_descriptors() {
        for id in ["", "../s", "s?url=evil", "s\n", "s;cmd", "deepseek:s"] { assert!(!valid_id(id)); }
        assert!(valid_id("sess_abc-123"));
        for value in [json!({"version":1,"port":0,"token":"a".repeat(64)}),
            json!({"version":1,"port":3,"token":"bad"}),
            json!({"version":2,"port":3,"token":"a".repeat(64)}),
            json!({"version":1,"port":3,"token":"a".repeat(64),"url":"https://example.com"})] {
            assert!(endpoint(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
    #[test]
    fn deepseek_success_requires_matching_renderer_receipt() {
        use std::{io::{Read, Write}, net::TcpListener};
        for matching in [true, false] {
            let server = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = Endpoint { version:1, port:server.local_addr().unwrap().port(), token:"a".repeat(64) };
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = server.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut bytes = Vec::new(); let mut buf = [0;1024];
                let header;
                loop { let n=stream.read(&mut buf).unwrap(); assert!(n>0); bytes.extend_from_slice(&buf[..n]);
                    if let Some(i)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {header=i+4;break;} }
                let text=String::from_utf8_lossy(&bytes[..header]);
                assert!(text.contains(&format!("Bearer {}", "a".repeat(64))));
                let length:usize=text.lines().find_map(|l|l.to_ascii_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse().unwrap())).unwrap();
                while bytes.len()<header+length {let n=stream.read(&mut buf).unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);}
                let request:Value=serde_json::from_slice(&bytes[header..header+length]).unwrap();
                assert_eq!(request["sessionId"],"sess_abc");
                let answer=json!({"ok":true,"sessionId":if matching {"sess_abc"}else{"wrong"},"requestId":request["requestId"]}).to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",answer.len(),answer).unwrap();
            });
            let client=Client::builder().no_proxy().timeout(Duration::from_secs(3)).build().unwrap();
            assert_eq!(navigate(&client,&endpoint,"sess_abc").is_ok(),matching);
            worker.join().unwrap();
        }
    }
}
