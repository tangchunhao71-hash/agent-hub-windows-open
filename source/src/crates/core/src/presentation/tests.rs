// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Ways back to a session: the desktop app's index, hook origins and the
//! session's log, and Claude Code's background jobs.
use crate::AgentHudCore;
use agent_hud_agents::HookEvent;
use chrono::Utc;
use serde_json::{json, Value};
use std::{fs, path::Path};

fn write(path: &Path, value: Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}
/// A fresh UUID-shaped session ID.
fn session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (t >> 32) as u32,
        (t >> 16) as u16,
        n & 0xfff,
        (t >> 4) & 0xfff,
        t & 0xffff_ffff_ffff
    )
}
fn core(home: &Path) -> AgentHudCore {
    let mut core = AgentHudCore::new(vec![], home.join("app"));
    core.provider_roots = vec![(home.join(".claude/projects"), "claude")];
    core
}
/// A Claude log whose lines name these clients, a second apart, the last now.
fn transcript(home: &Path, id: &str, entrypoints: &[&str]) {
    let now = Utc::now();
    let lines = entrypoints
        .iter()
        .enumerate()
        .map(|(i, entrypoint)| {
            let at = now - chrono::Duration::seconds((entrypoints.len() - 1 - i) as i64);
            json!({"sessionId":id,"timestamp":at.to_rfc3339(),"type":"assistant","entrypoint":entrypoint,
                "message":{"id":format!("m{i}"),"role":"assistant","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"output_tokens":5}}})
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let path = home
        .join(".claude/projects/work")
        .join(format!("{id}.jsonl"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, lines + "\n").unwrap();
}
fn desktop_tab(home: &Path, id: &str) -> String {
    let local = format!("local_{}", session_id());
    write(
        &home
            .join("Library/Application Support/Claude/claude-code-sessions/org/account")
            .join(format!("{local}.json")),
        json!({"sessionId":local,"cliSessionId":id,"isArchived":false}),
    );
    local
}
fn origin(id: &str, host: &str, target: Value, ago_ms: i64) -> HookEvent {
    HookEvent {
        source: "claude".into(),
        session_id: id.into(),
        at_ms: Utc::now().timestamp_millis() - ago_ms,
        kind: "origin".into(),
        data: json!({"hostVerified":true,"host":host,"navigationTarget":target}),
    }
}
fn target(core: &mut AgentHudCore, id: &str) -> Value {
    core.read_projection_sources();
    let snapshot = core.request("snapshot", json!({})).unwrap();
    snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["id"] == id)
        .map(|session| session["navigationTarget"].clone())
        .expect("the session is listed")
}

#[test]
fn deepseek_transcripts_project_exact_navigation_in_snapshot_list_and_detail() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".dsh/sessions");
    let path = root.join("sess_abc/session.jsonl");
    write(
        &path,
        json!({"type":"session","version":4,"id":"sess_abc","cwd":"/work","createdAt":Utc::now().to_rfc3339()}),
    );
    let mut lines = fs::read(&path).unwrap();
    lines.push(b'\n');
    fs::write(&path, lines).unwrap();
    let mut core = AgentHudCore::new(vec![], temp.path().join("app"));
    core.provider_roots = vec![(root, "deepseek")];
    let expected = json!({"kind":"deepseek-session","id":"sess_abc"});
    assert_eq!(target(&mut core, "deepseek:sess_abc"), expected);
    let list = core
        .request("sessions", json!({"source":"deepseek"}))
        .unwrap();
    assert_eq!(list["items"][0]["navigationTarget"], expected);
    let detail = core
        .request("session_detail", json!({"id":"deepseek:sess_abc"}))
        .unwrap();
    assert_eq!(detail["session"]["navigationTarget"], expected);
    drop(core);
    let mut reopened = AgentHudCore::new(vec![], temp.path().join("app"));
    reopened.provider_roots = vec![(temp.path().join(".dsh/sessions"), "deepseek")];
    assert_eq!(
        target(&mut reopened, "deepseek:sess_abc"),
        expected,
        "cached log heads must keep the target"
    );
}

#[test]
fn a_newer_desktop_log_keeps_the_desktop_target_a_stale_cli_origin_would_clear() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let mut core = core(home);

    // Started in a terminal with hooks on, then resumed in the desktop app.
    let resumed = session_id();
    let local = desktop_tab(home, &resumed);
    transcript(home, &resumed, &["cli", "claude-desktop"]);
    core.ingest_hook_events(vec![origin(&resumed, "cli", Value::Null, 60_000)]);
    assert_eq!(
        target(&mut core, &resumed),
        json!({"kind":"claude-desktop","id":local})
    );

    // The origin is newer than the log: it stands, and names no destination.
    let fresh = session_id();
    desktop_tab(home, &fresh);
    transcript(home, &fresh, &["cli", "claude-desktop"]);
    core.ingest_hook_events(vec![origin(&fresh, "cli", Value::Null, -30_000)]);
    assert_eq!(target(&mut core, &fresh), Value::Null);

    // Desktop origin, later resumed in a terminal: no pane can be guessed.
    let left = session_id();
    desktop_tab(home, &left);
    transcript(home, &left, &["claude-desktop", "cli"]);
    core.ingest_hook_events(vec![origin(
        &left,
        "desktop",
        json!({"kind":"claude-desktop","id":left}),
        60_000,
    )]);
    assert_eq!(target(&mut core, &left), Value::Null);
}

#[test]
fn a_background_job_returns_by_moving_into_the_desktop_app() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let config = home.join(".claude");
    let mut core = core(home);
    let id = session_id();
    let job = id[..8].to_owned();
    transcript(home, &id, &["cli", "cli"]);
    write(
        &config.join("jobs").join(&job).join("state.json"),
        json!({"state":"blocked","sessionId":id,"template":"bg"}),
    );
    write(
        &config.join("sessions/101.json"),
        json!({"pid":101,"sessionId":id,"kind":"bg","status":"idle","entrypoint":"cli"}),
    );
    // The job's own hooks: the command line, no destination.
    core.ingest_hook_events(vec![origin(&id, "cli", Value::Null, 30_000)]);
    assert_eq!(
        target(&mut core, &id),
        json!({"kind":"claude-background-job","id":id,"job":job})
    );

    // Moved: the job's process went, the desktop app took the session in,
    // before the desktop app wrote anything to its log.
    fs::remove_file(config.join("sessions/101.json")).unwrap();
    write(
        &config.join("sessions/202.json"),
        json!({"pid":202,"sessionId":id,"kind":"interactive","entrypoint":"claude-desktop"}),
    );
    let local = desktop_tab(home, &id);
    assert_eq!(
        target(&mut core, &id),
        json!({"kind":"claude-desktop","id":local})
    );

    // A terminal resumed it later: hooks decide, and that one named no pane.
    write(
        &config.join("sessions/303.json"),
        json!({"pid":303,"sessionId":id,"kind":"interactive","entrypoint":"cli"}),
    );
    core.ingest_hook_events(vec![origin(&id, "cli", Value::Null, 0)]);
    assert_eq!(target(&mut core, &id), Value::Null);
}
