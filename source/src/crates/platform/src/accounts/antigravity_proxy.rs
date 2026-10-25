// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Pure mapping of a read-only system proxy setting onto the usage child only.
use agent_hud_agents::account::EngineStart;
use std::collections::HashMap;

pub(super) fn failure() -> String {
    agent_hud_core::l10n::text(
        "无法读取或识别系统 HTTP 代理，Antigravity CLI 额度读取已停止",
        "System HTTP proxy is unavailable or unsupported; Antigravity CLI quota reading stopped",
    )
}

pub(super) fn applies(start: &EngineStart<'_>) -> bool {
    start.folder == "antigravity"
        && start.args == ["-p", "/usage", "--output-format", "json"]
        && start.executable.is_absolute()
        && start
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("agy.exe"))
}

fn endpoint(raw: &str) -> Result<String, String> {
    let value = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("http://{raw}")
    };
    let url = reqwest::Url::parse(&value).map_err(|_| failure())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(failure());
    }
    Ok(url.origin().ascii_serialization())
}

pub(super) fn project(
    env: &HashMap<String, String>,
    enabled: bool,
    server: &str,
) -> Result<Vec<(&'static str, String)>, String> {
    let (http, https) = if !enabled {
        (String::new(), String::new())
    } else {
        let raw = server.trim();
        if raw.is_empty() {
            return Err(failure());
        }
        if raw.contains('=') {
            let mut protocols = HashMap::new();
            for part in raw.split(';').filter(|part| !part.trim().is_empty()) {
                let (name, value) = part.split_once('=').ok_or_else(failure)?;
                protocols.insert(name.trim().to_lowercase(), value.trim());
            }
            (
                endpoint(protocols.get("http").ok_or_else(failure)?)?,
                endpoint(protocols.get("https").ok_or_else(failure)?)?,
            )
        } else {
            let value = endpoint(raw)?;
            (value.clone(), value)
        }
    };
    let mut bypass = Vec::<String>::new();
    for value in env
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("NO_PROXY"))
        .map(|(_, value)| value)
    {
        for item in value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if !bypass.iter().any(|old| old == item) {
                bypass.push(item.to_owned());
            }
        }
    }
    for local in ["localhost", "127.0.0.1", "::1"] {
        if !bypass.iter().any(|old| old == local) {
            bypass.push(local.to_owned());
        }
    }
    // Empty values clear inherited stale proxy routes in this child. Explicitly
    // set both spellings for clients that inspect either case; Windows itself
    // treats these names case-insensitively.
    Ok(vec![
        ("HTTP_PROXY", http.clone()),
        ("http_proxy", http),
        ("HTTPS_PROXY", https.clone()),
        ("https_proxy", https),
        ("ALL_PROXY", String::new()),
        ("all_proxy", String::new()),
        ("NO_PROXY", bypass.join(",")),
        ("no_proxy", bypass.join(",")),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn values(result: Vec<(&'static str, String)>) -> HashMap<&'static str, String> {
        result.into_iter().collect()
    }

    #[test]
    fn usage_child_only_receives_the_system_http_routes() {
        let env = HashMap::from([
            ("HTTP_PROXY".into(), "http://old.invalid:1".into()),
            ("all_proxy".into(), "socks5://old.invalid:2".into()),
            ("no_proxy".into(), "example.test,127.0.0.1".into()),
        ]);
        let next = values(project(&env, true, "127.0.0.1:10808").unwrap());
        assert_eq!(next["HTTP_PROXY"], "http://127.0.0.1:10808");
        assert_eq!(next["https_proxy"], next["HTTP_PROXY"]);
        assert_eq!(next["ALL_PROXY"], "");
        assert_eq!(next["all_proxy"], "");
        assert_eq!(next["NO_PROXY"], "example.test,127.0.0.1,localhost,::1");
        assert_eq!(env["HTTP_PROXY"], "http://old.invalid:1");
        let off = values(project(&env, false, "ignored").unwrap());
        for key in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            assert_eq!(off[key], "");
        }
        assert_eq!(off["NO_PROXY"], next["NO_PROXY"]);
    }

    #[test]
    fn per_protocol_routes_require_supported_endpoints_without_credentials() {
        let env = HashMap::new();
        let next = values(
            project(
                &env,
                true,
                "http=127.0.0.1:10001;https=https://proxy.test:10002",
            )
            .unwrap(),
        );
        assert_eq!(next["HTTP_PROXY"], "http://127.0.0.1:10001");
        assert_eq!(next["HTTPS_PROXY"], "https://proxy.test:10002");
        for invalid in [
            "",
            "http=proxy.test:80",
            "socks5://proxy.test:1080",
            "http://user:fixture-secret@proxy.test:80",
            "http://proxy.test/path",
            "http://proxy.test/?x=1",
            "http://proxy.test/#fragment",
        ] {
            let error = project(&env, true, invalid).unwrap_err();
            assert!(!error.contains("fixture-secret"));
            assert!(!error.contains("proxy.test"));
        }
    }

    #[test]
    fn unrelated_engines_and_prompts_are_not_proxy_targets() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("agy.exe");
        let mut start = EngineStart {
            executable: &exe,
            args: &["-p", "/usage", "--output-format", "json"],
            folder: "antigravity",
            env: &[],
        };
        assert!(applies(&start));
        start.folder = "codex";
        assert!(!applies(&start));
        start.folder = "antigravity";
        start.args = &["-p", "prompt"];
        assert!(!applies(&start));
        start.args = &["-p", "/usage", "--output-format", "json"];
        start.executable = std::path::Path::new("agy.exe");
        assert!(!applies(&start));
        let other = root.path().join("other.exe");
        start.executable = &other;
        assert!(!applies(&start));
    }
}
