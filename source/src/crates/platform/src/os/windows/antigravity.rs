// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Windows local-service discovery. No server is launched or signed in.
//! Candidate command lines (including CSRF tokens) stay in memory.
use super::super::ProcessInfo;
use serde::Deserialize;
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Candidate {
    pid: u32,
    created: u64,
    executable: String,
    command: String,
}
static IDENTITIES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, u64>>> =
    std::sync::OnceLock::new();
const LIMIT: usize = 1024 * 1024;
const UNAVAILABLE: &str = "Antigravity process discovery is unavailable";

pub(crate) fn processes() -> Result<Vec<ProcessInfo>, String> {
    let system = std::env::var_os("SystemRoot").ok_or(UNAVAILABLE)?;
    let mut command =
        Command::new(PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    super::configure_command(&mut command);
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(include_str!(
            "../../../scripts/windows/antigravity-processes.ps1"
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| UNAVAILABLE)?;
    let stdout = child.stdout.take().ok_or(UNAVAILABLE)?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = stdout.take((LIMIT + 1) as u64).read_to_end(&mut bytes);
        let _ = send.send(read.map(|_| bytes));
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(UNAVAILABLE.into());
            }
        }
    }
    let bytes = receive
        .recv_timeout(Duration::from_millis(100))
        .map_err(|_| UNAVAILABLE)?
        .map_err(|_| UNAVAILABLE)?;
    candidates(&bytes)
}
fn candidates(bytes: &[u8]) -> Result<Vec<ProcessInfo>, String> {
    if bytes.len() > LIMIT {
        return Err(UNAVAILABLE.into());
    }
    let mut result = Vec::new();
    let mut identities = std::collections::HashMap::new();
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        if line.last() != Some(&b'\n') {
            return Err(UNAVAILABLE.into());
        }
        let candidate: Candidate = serde_json::from_slice(line).map_err(|_| UNAVAILABLE)?;
        if !std::path::Path::new(&candidate.executable).is_absolute()
            || !related(&candidate.executable)
        {
            continue;
        }
        let Some(_process) = super::process::owned_process(candidate.pid, Some(candidate.created))
        else {
            continue;
        };
        identities.insert(candidate.pid, candidate.created);
        result.push(ProcessInfo {
            pid: candidate.pid,
            executable: candidate.executable,
            command: candidate.command,
        });
    }
    *IDENTITIES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| UNAVAILABLE)? = identities;
    Ok(result)
}
fn related(executable: &str) -> bool {
    let executable = executable.replace('\\', "/").to_ascii_lowercase();
    let name = executable.rsplit('/').next().unwrap_or_default();
    name == "agy.exe"
        || ((name.starts_with("language_server") || name.starts_with("language-server"))
            && name.ends_with(".exe")
            && [
                "/antigravity/",
                "/antigravity ide/",
                "/antigravity-cli/",
                "/antigravity_cli/",
            ]
            .iter()
            .any(|part| executable.contains(part)))
}

#[link(name = "iphlpapi")]
unsafe extern "system" {
    fn GetExtendedTcpTable(
        table: *mut std::ffi::c_void,
        size: *mut u32,
        order: i32,
        family: u32,
        class: i32,
        reserved: u32,
    ) -> u32;
}
// MIB_TCPTABLE_OWNER_PID: DWORD count followed by rows of six DWORD fields.
fn ports(words: &[u32], pid: u32) -> Result<Vec<u16>, String> {
    let count = *words.first().ok_or(UNAVAILABLE)? as usize;
    if count > (words.len() - 1) / 6 {
        return Err(UNAVAILABLE.into());
    }
    let mut result = Vec::new();
    for row in words[1..1 + count * 6].chunks_exact(6) {
        if row[0] == 2 && row[1].to_ne_bytes() == [127, 0, 0, 1] && row[5] == pid {
            let port = u16::from_be(row[2] as u16);
            if port != 0 {
                result.push(port);
            }
        }
    }
    result.sort_unstable();
    result.dedup();
    Ok(result)
}
pub(crate) fn listening_ports(pid: u32) -> Result<Vec<u16>, String> {
    // Hold a verified process handle while reading the table, preventing PID
    // reuse during discovery. The service connects only to 127.0.0.1.
    let created = *IDENTITIES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| UNAVAILABLE)?
        .get(&pid)
        .ok_or(UNAVAILABLE)?;
    let _process = super::process::owned_process(pid, Some(created)).ok_or(UNAVAILABLE)?;
    tcp_ports(pid)
}
fn tcp_ports(pid: u32) -> Result<Vec<u16>, String> {
    let mut size = 0;
    let first = unsafe { GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, 2, 3, 0) };
    if first != 122 && first != 0 {
        return Err(UNAVAILABLE.into());
    }
    for _ in 0..3 {
        if size < 4 || size > 16 * 1024 * 1024 {
            return Err(UNAVAILABLE.into());
        }
        let mut words = vec![0u32; (size as usize).div_ceil(4)];
        let status =
            unsafe { GetExtendedTcpTable(words.as_mut_ptr().cast(), &mut size, 0, 2, 3, 0) };
        if status == 122 {
            continue;
        }
        if status != 0 {
            return Err(UNAVAILABLE.into());
        }
        return ports(&words[..(size as usize / 4).min(words.len())], pid);
    }
    Err(UNAVAILABLE.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn antigravity_windows_paths_select_only_known_servers() {
        assert!(related(
            r"C:\Users\用户\AppData\Local\Programs\Antigravity\resources\bin\language_server.exe"
        ));
        assert!(related(
            r"C:\Apps\Antigravity IDE\resources\app\extensions\antigravity\bin\language_server_windows_x64.exe"
        ));
        assert!(related(r"C:\Apps\AGY.EXE"));
        assert!(!related(r"C:\Other\language_server.exe"));
        assert!(!related(r"C:\Antigravity\not-language_server.exe"));
        assert!(!related(r"C:\Antigravity\language_server.exe.bak"));
    }
    #[test]
    fn antigravity_tcp_table_filters_pid_state_address_and_duplicates() {
        let row = |pid, state, address: [u8; 4], port: u16| {
            [
                state,
                u32::from_ne_bytes(address),
                u32::from(port.to_be()),
                0,
                0,
                pid,
            ]
        };
        let mut words = vec![6];
        for data in [
            row(4, 2, [127, 0, 0, 1], 1234),
            row(5, 2, [127, 0, 0, 1], 4567),
            row(4, 5, [127, 0, 0, 1], 6789),
            row(4, 2, [0, 0, 0, 0], 1111),
            row(4, 2, [192, 168, 1, 1], 2222),
            row(4, 2, [127, 0, 0, 1], 1234),
        ] {
            words.extend(data);
        }
        assert_eq!(ports(&words, 4).unwrap(), [1234]);
        assert!(ports(&[], 4).is_err());
        assert!(ports(&[1, 2, 3], 4).is_err());
        assert!(ports(&[u32::MAX], 4).is_err());
    }
    #[test]
    fn antigravity_native_table_finds_an_isolated_loopback_listener() {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        assert!(tcp_ports(std::process::id()).unwrap().contains(&port));
        assert!(!tcp_ports(0).unwrap().contains(&port));
        assert!(listening_ports(0).is_err());
    }
    #[test]
    fn antigravity_candidates_reject_malformed_or_unverified_records_without_secrets() {
        for bytes in [b"invalid csrf-secret".as_slice(), b"{}", b"{}\n"] {
            assert_eq!(candidates(bytes).err().as_deref(), Some(UNAVAILABLE));
        }
        let record = serde_json::json!({"pid":0,"created":1,"executable":"C:\\Antigravity\\language_server.exe","command":"--csrf_token fixture-secret"}).to_string()+"\n";
        assert!(candidates(record.as_bytes()).unwrap().is_empty());
    }
}
