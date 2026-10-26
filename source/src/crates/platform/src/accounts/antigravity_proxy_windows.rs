// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Current-user system HTTP proxy, read-only and scoped to the /usage child.
use super::{antigravity_proxy, AccountContext};
use agent_hud_agents::account::EngineStart;
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const LIMIT: u64 = 64 * 1024;

pub(super) fn installed_cli_root(home: &Path, env: &HashMap<String, String>) -> Option<PathBuf> {
    Some(
        crate::os::variable(env, "LOCALAPPDATA", home)
            .unwrap_or_else(|| home.join("AppData/Local")),
    )
}

fn registry_value(output: &[u8], name: &str) -> Result<String, String> {
    let text = std::str::from_utf8(output).map_err(|_| antigravity_proxy::failure())?;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if !fields
            .next()
            .is_some_and(|field| field.eq_ignore_ascii_case(name))
        {
            continue;
        }
        let kind = fields.next().ok_or_else(antigravity_proxy::failure)?;
        if (name == "ProxyEnable" && kind != "REG_DWORD")
            || (name == "ProxyServer" && !matches!(kind, "REG_SZ" | "REG_EXPAND_SZ"))
        {
            return Err(antigravity_proxy::failure());
        }
        return Ok(fields.collect::<Vec<_>>().join(" "));
    }
    Err(antigravity_proxy::failure())
}

fn read_setting(context: &AccountContext, name: &str) -> Result<String, String> {
    let system = crate::os::variable(&context.env, "SystemRoot", &context.home)
        .filter(|path| path.is_absolute())
        .ok_or_else(antigravity_proxy::failure)?;
    if context.cancelled.load(Ordering::Acquire) {
        return Err(antigravity_proxy::failure());
    }
    let mut command = Command::new(system.join("System32/reg.exe"));
    crate::os::configure_command(&mut command);
    command
        .args(["query", KEY, "/v", name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| antigravity_proxy::failure())?;
    let stdout = child.stdout.take().ok_or_else(antigravity_proxy::failure)?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(LIMIT + 1).read_to_end(&mut bytes);
        let _ = send.send(result.map(|_| bytes));
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if context.cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(antigravity_proxy::failure());
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(antigravity_proxy::failure());
            }
        }
    }
    let bytes = receive
        .recv_timeout(Duration::from_millis(100))
        .map_err(|_| antigravity_proxy::failure())?
        .map_err(|_| antigravity_proxy::failure())?;
    if bytes.len() > LIMIT as usize {
        return Err(antigravity_proxy::failure());
    }
    registry_value(&bytes, name)
}

pub(super) fn environment<'a>(
    context: &AccountContext,
    start: &EngineStart<'a>,
) -> Result<Vec<(&'a str, String)>, String> {
    let mut result = start.env.to_vec();
    if !antigravity_proxy::applies(start) {
        return Ok(result);
    }
    let enabled = read_setting(context, "ProxyEnable")?;
    let enabled = match enabled.as_str() {
        "0" | "0x0" => false,
        "1" | "0x1" => true,
        _ => return Err(antigravity_proxy::failure()),
    };
    let server = if enabled {
        read_setting(context, "ProxyServer")?
    } else {
        String::new()
    };
    result.extend(antigravity_proxy::project(&context.env, enabled, &server)?);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_expected_registry_values_are_accepted() {
        assert_eq!(
            registry_value(
                b"HKEY_CURRENT_USER\\example\n    ProxyEnable    REG_DWORD    0x1\n",
                "ProxyEnable"
            )
            .unwrap(),
            "0x1"
        );
        assert_eq!(
            registry_value(
                b"    ProxyServer    REG_SZ    http=proxy.test:80;https=proxy.test:81\n",
                "ProxyServer"
            )
            .unwrap(),
            "http=proxy.test:80;https=proxy.test:81"
        );
        assert!(registry_value(b"    ProxyEnable    REG_SZ    1\n", "ProxyEnable").is_err());
        assert!(registry_value(
            b"    Unexpected    REG_SZ    fixture-secret\n",
            "ProxyServer"
        )
        .unwrap_err()
        .find("fixture-secret")
        .is_none());
    }
}
