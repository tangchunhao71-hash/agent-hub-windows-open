use super::Endpoint;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, process::{Child, Command, Stdio}, time::{Duration, Instant}};

#[cfg(windows)]
#[path = "process_windows.rs"]
mod process;
#[cfg(not(windows))]
#[path = "process_unix.rs"]
mod process;

const OUTPUT_LIMIT: u64 = 1_048_576;
pub(super) struct Outcome {
    pub state: &'static str,
    pub message: String,
    pub session: Option<String>,
}
impl Outcome {
    fn failed(message: &str) -> Self { Self { state: "failed", message: message.into(), session: None } }
    fn unknown(message: &str) -> Self { Self { state: "deliveryUnknown", message: message.into(), session: None } }
}

/// Fingerprint only stable account identity, not rotating bearer/refresh tokens.
/// The actual credential is read by the existing CLI, never passed on its command line.
pub(super) fn login_scope(source: &str, file: &Path) -> Result<String, String> {
    if file.with_file_name(format!("{}.logged-out", file.file_name().unwrap_or_default().to_string_lossy())).exists() {
        return Err(format!("{source}: existing login is logged out"));
    }
    let bytes = fs::read(file).map_err(|_| format!("{source}: existing login file is unavailable"))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| format!("{source}: login file cannot be read"))?;
    let identity = match source {
        "codebuddy" => {
            let uid = value["account"]["uid"].as_str().filter(|v| !v.is_empty()).ok_or("CodeBuddy account identity missing")?;
            if value["auth"]["accessToken"].is_null() || value["auth"]["accessToken"] == "" { return Err("CodeBuddy login credential missing".into()); }
            json!([source, uid, value["account"]["enterpriseId"], value["account"]["type"], value["auth"]["domain"]])
        }
        "codex" => {
            if value["auth_mode"] != "chatgpt" { return Err("Codex must use the existing ChatGPT login".into()); }
            let account = value["tokens"]["account_id"].as_str().filter(|v| !v.is_empty()).ok_or("Codex account identity missing")?;
            if value["tokens"]["access_token"].as_str().unwrap_or("").is_empty() { return Err("Codex login credential missing".into()); }
            json!([source, account])
        }
        _ => return Err("Unsupported login source".into()),
    };
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&identity).map_err(|e| e.to_string())?)))
}

pub(super) fn command(source: &str, endpoint: &Endpoint, work: &Path, task: &Value) -> Command {
    let mut command = Command::new(&endpoint.program);
    if let Some(script) = &endpoint.script { command.arg(script); }
    // Keep OS/network process configuration, but never inherit an API key, a parent
    // session, a product endpoint override, or permission/MCP overrides from a caller.
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if name.starts_with("CODEBUDDY_") || name.starts_with("ACC_")
            || name.starts_with("ANTHROPIC_") || name.starts_with("OPENAI_")
            || name.starts_with("CLAUDE_") || name.starts_with("AGENTHUD_")
            || matches!(name.as_str(), "AUTH_TOKEN" | "CODEX_THREAD_ID" | "CODEX_API_KEY" | "CODEX_ACCESS_TOKEN" | "NODE_OPTIONS") {
            command.env_remove(&key);
        }
    }
    command.env("AGENTHUD_HELPER", "1");
    if source == "codebuddy" {
        command.args(["--print", "--output-format", "json", "--tools", "", "--strict-mcp-config",
            "--mcp-config", "{\"mcpServers\":{}}", "--setting-sources", "none", "--max-turns", "1",
            "--no-session-persistence", "--session-id"])
            .arg(task["helperSessionId"].as_str().unwrap_or_default())
            .args(["--system-prompt", "Complete only the supplied text task. Do not use tools, access files, delegate, or contact other agents. Return the answer as text."])
            .env("CODEBUDDY_DISABLE_AUTO_MEMORY", "1")
            .env("CODEBUDDY_DISABLE_PARENT_MEMORY", "1")
            .env("CODEBUDDY_SKIP_BUILTIN_MARKETPLACE", "1");
    } else {
        command.args(["exec", "--ignore-user-config", "--ephemeral", "--skip-git-repo-check", "--sandbox", "read-only",
            "--json", "--color", "never", "--model", "gpt-6.1-sol", "-C"])
            .arg(work)
            .args(["-c", "model_reasoning_effort=\"low\"", "-c", "approval_policy=\"never\"",
                "-c", "project_doc_max_bytes=0", "-c", "web_search=\"disabled\"",
                "-c", "tools.update_plan.enabled=false",
                "-c", "features={shell_tool=false,view_image=false,apps=false,plugins=false,hooks=false,multi_agent=false,multi_agent_v2=false,memories=false,browser_use=false,computer_use=false,image_generation=false,code_mode_host=false,code_mode=false}",
                "-c", "developer_instructions=\"Complete only the supplied text task. Do not use tools, access files, delegate, or contact other agents. Return the answer as text.\""]);
    }
    command.arg("--").arg(task["prompt"].as_str().unwrap_or_default()).current_dir(work).stdin(Stdio::null());
    process::hide(&mut command);
    command
}

fn stop(child: &mut Child) {
    process::stop_tree(child);
    let _ = child.kill();
    let _ = child.wait();
}

pub(super) fn execute(source: &str, endpoint: &Endpoint, directory: &Path, task: &Value, cancelled: impl Fn() -> bool) -> Outcome {
    let work = directory.join(task["taskId"].as_str().unwrap_or("invalid"));
    if fs::create_dir(&work).is_err() { return Outcome::failed("Not delivered: task directory already exists or is unavailable; inspect receipt"); }
    let output = work.join("stdout.json");
    let error = work.join("stderr.txt");
    let (Ok(stdout), Ok(stderr)) = (fs::File::create(&output), fs::File::create(&error)) else {
        return Outcome::failed("Not delivered: unable to create private output files");
    };
    if cancelled() { return Outcome::failed("Not delivered: cancelled before process start"); }
    let mut command = command(source, endpoint, &work, task);
    command.stdout(stdout).stderr(stderr);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return Outcome::failed("Not delivered: unable to start the configured CLI"),
    };
    let started = Instant::now();
    loop {
        if cancelled() || started.elapsed() >= Duration::from_secs(120)
            || [&output, &error].iter().any(|p| fs::metadata(p).map(|m| m.len() > OUTPUT_LIMIT).unwrap_or(true)) {
            stop(&mut child);
            return Outcome::unknown("Local helper stopped (cancel, timeout, login change or output limit). Remote completion is unknown; do not replay.");
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() { return Outcome::unknown("CLI exited without verified success; inspect private stderr. Do not replay."); }
                let mut bytes = Vec::new();
                if fs::File::open(&output).and_then(|f| f.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)).is_err() || bytes.len() as u64 > OUTPUT_LIMIT {
                    return Outcome::unknown("Invalid or oversized result; do not replay.");
                }
                return terminal_result(source, task["helperSessionId"].as_str().unwrap_or_default(), &bytes);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => { stop(&mut child); return Outcome::unknown("Could not observe helper exit; do not replay."); }
        }
    }
}

pub(super) fn terminal_result(source: &str, expected_session: &str, bytes: &[u8]) -> Outcome {
    let invalid = || Outcome::unknown("No matching successful terminal receipt; do not replay.");
    if source == "codebuddy" {
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else { return invalid(); };
        // Current headless JSON output is a transcript array with one final result.
        // Accept text messages only, never silently discard tool/extra-result events.
        let value = if let Some(events) = value.as_array() {
            let Some((last, messages)) = events.split_last() else { return invalid(); };
            if messages.iter().any(|event| event["type"] != "message"
                || !matches!(event["role"].as_str(), Some("user" | "assistant"))
                || event["sessionId"] != expected_session
                || event["content"].as_array().is_none_or(|content| content.iter().any(|part|
                    !matches!(part["type"].as_str(), Some("input_text" | "output_text"))))) {
                return invalid();
            }
            if messages.iter().filter(|event| event["role"] == "assistant").count() != 1
                || messages.iter().filter(|event| event["role"] == "user").count() != 1 {
                return invalid();
            }
            last
        } else { &value };
        if value["type"] == "result" && value["subtype"] == "success" && value["is_error"] != true
            && value["session_id"] == expected_session
            // This CLI counts active history entries (user + assistant), not model calls.
            && (value["num_turns"].is_null() || matches!(value["num_turns"].as_u64(), Some(1 | 2))) {
            if let Some(text) = value["result"].as_str().filter(|s| !s.trim().is_empty()) {
                return Outcome { state: "completed", message: text.into(), session: Some(expected_session.into()) };
            }
        }
        return invalid();
    }
    let mut session = None;
    let mut text = None;
    let mut started = 0;
    let mut completed = 0;
    for line in bytes.split(|b| *b == b'\n').filter(|l| !l.iter().all(u8::is_ascii_whitespace)) {
        let Ok(value) = serde_json::from_slice::<Value>(line) else { return invalid(); };
        match value["type"].as_str() {
            Some("thread.started") if session.is_none() => session = value["thread_id"].as_str().filter(|s| !s.is_empty()).map(str::to_owned),
            Some("turn.started") if session.is_some() && completed == 0 => started += 1,
            Some("turn.completed") if started == 1 && text.is_some() => completed += 1,
            Some("item.started" | "item.completed" | "item.updated") if completed == 0 => {
                match value["item"]["type"].as_str() {
                    Some("agent_message") => {
                        if value["type"] == "item.completed" { text = value["item"]["text"].as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned); }
                    }
                    Some("reasoning") => {},
                    // Tool invocations, extra turns and error events are not accepted
                    // as successful text-only delegation.
                    _ => return invalid(),
                }
            }
            _ => return invalid(),
        }
    }
    if started == 1 && completed == 1 && session.is_some() && text.is_some() {
        Outcome { state: "completed", message: text.unwrap_or_default(), session }
    } else { invalid() }
}
