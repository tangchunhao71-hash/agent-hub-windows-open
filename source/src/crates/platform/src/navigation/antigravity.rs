//! The public AntigravityNavigation contract: an already-running desktop's
//! loopback CDP inventory and /c/<UUID>. Never launches an application/server.
use agent_hud_core::l10n;
use reqwest::{blocking::Client, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};
const LIMIT: usize = 1024 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Page {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    web_socket_debugger_url: Option<String>,
}
pub(super) struct Destination {
    id: String,
    url: Url,
    socket: Url,
}
fn eligible(id: &str, page: &Page, port: u16) -> Option<Destination> {
    if !super::uuid(id) || page.kind != "page" {
        return None;
    }
    let mut url = Url::parse(&page.url).ok()?;
    let socket = Url::parse(page.web_socket_debugger_url.as_deref()?).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("127.0.0.1")
        || socket.scheme() != "ws"
        || socket.host_str() != Some("127.0.0.1")
        || socket.port() != Some(port)
        || socket.path() != format!("/devtools/page/{}", page.id)
    {
        return None;
    }
    // User info must not turn a loopback navigation into a credential-bearing request.
    if !url.username().is_empty()
        || url.password().is_some()
        || !socket.username().is_empty()
        || socket.password().is_some()
    {
        return None;
    }
    url.set_path(&format!("/c/{id}"));
    url.set_query(None);
    url.set_fragment(None);
    Some(Destination {
        id: page.id.clone(),
        url,
        socket,
    })
}
pub(super) fn destination(id: &str, pages: &[Page], port: u16) -> Option<Destination> {
    let mut all = pages
        .iter()
        .filter_map(|page| eligible(id, page, port))
        .collect::<Vec<_>>();
    let existing = all
        .iter()
        .enumerate()
        .filter(|(_, target)| {
            pages
                .iter()
                .find(|p| p.id == target.id)
                .and_then(|p| Url::parse(&p.url).ok())
                .is_some_and(|u| u.path() == target.url.path())
        })
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if existing.len() == 1 {
        return Some(all.swap_remove(existing[0]));
    }
    if existing.is_empty() && all.len() == 1 {
        all.pop()
    } else {
        None
    }
}
fn remaining(deadline: Instant) -> Result<Duration, String> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(l10n::text(
            "打开 Antigravity 会话超时",
            "Antigravity navigation timed out",
        ))
    } else {
        Ok(left)
    }
}
fn inventory(client: &Client, port: u16, deadline: Instant) -> Result<Vec<Page>, String> {
    let response = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .timeout(remaining(deadline)?.min(Duration::from_secs(2)))
        .send()
        .map_err(|_| "Antigravity page inventory is unavailable")?;
    if response.status() != 200 {
        return Err("Antigravity page inventory is unavailable".into());
    }
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err("Antigravity page inventory exceeds its size limit".into());
    }
    let mut bytes = Vec::new();
    response
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Antigravity page inventory is unavailable")?;
    if bytes.len() > LIMIT {
        return Err("Antigravity page inventory exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Antigravity page inventory is invalid".into())
}
fn activate_page(client: &Client, port: u16, id: &str, deadline: Instant) -> Result<bool, String> {
    let mut url = Url::parse(&format!("http://127.0.0.1:{port}/json/activate/")).unwrap();
    url.path_segments_mut().unwrap().pop_if_empty().push(id);
    client.get(url).timeout(remaining(deadline)?).send()
        .map(|response| response.status() == 200)
        .map_err(|_| "Antigravity page activation is unavailable".into())
}
pub(super) fn navigate(id: &str, port: u16) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| "Antigravity navigation client is unavailable")?;
    let pages = inventory(&client, port, deadline)?;
    let Some(target) = destination(id, &pages, port) else {
        return Ok(false);
    };
    if pages
        .iter()
        .find(|p| p.id == target.id)
        .and_then(|p| Url::parse(&p.url).ok())
        .is_some_and(|u| u.path() == target.url.path())
    {
        return activate_page(&client, port, &target.id, deadline);
    }
    let stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        remaining(deadline)?.min(Duration::from_secs(2)),
    )
    .map_err(|_| "Antigravity navigation channel is unavailable")?;
    stream
        .set_read_timeout(Some(remaining(deadline)?))
        .map_err(|_| "Antigravity navigation channel is unavailable")?;
    stream
        .set_write_timeout(Some(remaining(deadline)?))
        .map_err(|_| "Antigravity navigation channel is unavailable")?;
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(LIMIT))
        .max_frame_size(Some(LIMIT))
        .read_buffer_size(8192)
        .write_buffer_size(8192);
    let (mut socket, _) =
        tungstenite::client::client_with_config(target.socket.as_str(), stream, Some(config))
            .map_err(|_| "Antigravity navigation handshake failed")?;
    socket
        .send(tungstenite::Message::Text(
            json!({"id":1,"method":"Page.navigate","params":{"url":target.url.as_str()}})
                .to_string()
                .into(),
        ))
        .map_err(|_| "Antigravity navigation request failed")?;
    loop {
        socket
            .get_mut()
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|_| "Antigravity navigation channel is unavailable")?;
        let message = socket
            .read()
            .map_err(|_| "Antigravity navigation response is unavailable")?;
        if !matches!(
            message,
            tungstenite::Message::Text(_) | tungstenite::Message::Binary(_)
        ) {
            continue;
        }
        let Ok(response) = serde_json::from_slice::<Value>(&message.into_data()) else {
            continue;
        };
        if response["id"] != 1 {
            continue;
        }
        if !response["result"]["frameId"].is_string()
            || response["result"].get("errorText").is_some()
        {
            return Ok(false);
        }
        break;
    }
    drop(socket);
    loop {
        let current = inventory(&client, port, deadline)?;
        if current
            .iter()
            .find(|page| page.id == target.id)
            .and_then(|page| Url::parse(&page.url).ok())
            .is_some_and(|url| {
                url.scheme() == target.url.scheme()
                    && url.host_str() == target.url.host_str()
                    && url.port() == target.url.port()
                    && url.path() == target.url.path()
            })
        {
            return activate_page(&client, port, &target.id, deadline);
        }
        std::thread::sleep(remaining(deadline)?.min(Duration::from_millis(100)));
    }
}
const APP: &str = "com.google.antigravity";
// Windows parity: a drive path is a URI path, never a URL authority.
fn workspace_uri(cwd: Option<&str>) -> Option<String> {
    let cwd = cwd?;
    if cwd.is_empty() || cwd.chars().any(char::is_control) {
        return None;
    }
    let bytes = cwd.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\');
    let path = if drive {
        format!("/{}", cwd.replace('\\', "/"))
    } else if cwd.starts_with('/') && !cwd.starts_with("//") && !cwd.contains('\\') {
        cwd.to_owned()
    } else {
        return None;
    };
    let mut url = Url::parse("antigravity://file").ok()?;
    // set_path escapes spaces, Unicode and URL delimiters; escape literal %
    // first so a folder called %2F cannot become a path separator.
    url.set_path(&path.replace('%', "%25"));
    Some(url.into())
}
pub(super) fn open(home: &Path, id: &str, cwd: Option<&str>) -> Result<bool, String> {
    if !crate::os::ACTIVATES_APPS {
        return Err(l10n::text(
            "此系统暂不支持打开 Antigravity 会话",
            "Antigravity native session navigation is not implemented for this operating system",
        ));
    }
    if !crate::os::app_running(APP) {
        return Ok(false);
    }
    let mut opened = false;
    if super::uuid(id) {
        let path = crate::os::app_support(home, &std::env::vars().collect())
            .join("Antigravity/DevToolsActivePort");
        if let Ok(file) = std::fs::File::open(path) {
            let mut bytes = Vec::new();
            if file.take(4097).read_to_end(&mut bytes).is_ok() && bytes.len() <= 4096 {
                if let Some(port) = std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|s| s.lines().next())
                    .and_then(|s| s.parse::<u16>().ok())
                    .filter(|p| *p > 0)
                {
                    if let Ok(true) = navigate(id, port) {
                        opened = true;
                    }
                }
            }
        }
    }
    if !opened {
        if let Some(uri) = workspace_uri(cwd) {
            if crate::os::open_session_url(&uri).is_ok() {
                return Err(l10n::text(
                    "已请求打开 Antigravity 工作区，但未确认返回目标会话",
                    "Requested the Antigravity workspace, but could not confirm the target conversation",
                ));
            }
        }
        return Ok(false);
    }
    Ok(crate::os::activate_app(APP))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "00000000-0000-4000-8000-000000000001";
    #[test]
    fn workspace_paths_preserve_unicode_spaces_and_literal_url_characters() {
        assert_eq!(workspace_uri(Some(r"C:\Users\测试\My Project")),
            Some("antigravity://file/C:/Users/%E6%B5%8B%E8%AF%95/My%20Project".into()));
        assert_eq!(workspace_uri(Some("D:/work/a%2Fb#c?d")),
            Some("antigravity://file/D:/work/a%252Fb%23c%3Fd".into()));
        assert_eq!(workspace_uri(Some("/Users/me/project")),
            Some("antigravity://file/Users/me/project".into()));
        for invalid in ["", "relative/path", "C:relative", r"\\server\share", "//server/share", "https://example.com", "C:/line\nbreak"] {
            assert!(workspace_uri(Some(invalid)).is_none(), "{invalid:?}");
        }
    }
    fn page(id: &str, path: &str) -> Page {
        Page { id: id.into(), kind: "page".into(), url: format!("https://127.0.0.1:9000/{path}"),
            web_socket_debugger_url: Some(format!("ws://127.0.0.1:9222/devtools/page/{id}")) }
    }
    #[test]
    fn cdp_destination_requires_unambiguous_local_page() {
        assert!(destination(ID, &[page("one", ""), page("two", "")], 9222).is_none());
        assert_eq!(destination(ID, &[page("one", ""), page("two", &format!("c/{ID}"))], 9222).unwrap().id, "two");
        let mut remote = page("one", "");
        remote.web_socket_debugger_url = Some("ws://example.com:9222/devtools/page/one".into());
        assert!(destination(ID, &[remote], 9222).is_none());
        assert!(destination("invalid", &[page("one", "")], 9222).is_none());
        assert!(destination(ID, &[page("one", "")], 9999).is_none());
    }
    #[test]
    fn already_open_conversation_still_activates_its_cdp_page() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                paths.push(line.split_whitespace().nth(1).unwrap().to_owned());
                loop { line.clear(); reader.read_line(&mut line).unwrap(); if line == "\r\n" { break; } }
                let body = if paths.len() == 1 {
                    json!([{"id":"page-one","type":"page","url":format!("https://127.0.0.1:9000/c/{ID}"),
                        "webSocketDebuggerUrl":format!("ws://127.0.0.1:{port}/devtools/page/page-one")}]).to_string()
                } else { "Target activated".into() };
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            paths
        });
        assert!(navigate(ID, port).unwrap());
        assert_eq!(server.join().unwrap(), ["/json/list", "/json/activate/page-one"]);
    }
}
