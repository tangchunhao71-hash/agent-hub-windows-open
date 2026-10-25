// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
use super::*;

fn fixture() -> (tempfile::TempDir, Arc<Collaboration>, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let node = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .flat_map(|p| [p.join("node.exe"), p.join("node")])
        .find(|p| p.is_file())
        .expect("Node required for CLI fixture");
    let script = dir.path().join("fixture.cjs");
    fs::write(&script, r#"const a=process.argv;const id=a[a.indexOf('--session-id')+1];if(a.includes('exec')){for(const e of [{type:'thread.started',thread_id:'fixture-codex-thread'},{type:'turn.started'},{type:'item.completed',item:{type:'agent_message',text:'fixture result'}},{type:'turn.completed'}])console.log(JSON.stringify(e));}else console.log(JSON.stringify({type:'result',subtype:'success',session_id:id,result:'fixture result'}));"#).unwrap();
    let buddy = dir.path().join("buddy-login.json");
    fs::write(
        &buddy,
        r#"{"account":{"uid":"fixture-buddy"},"auth":{"accessToken":"fixture-token"}}"#,
    )
    .unwrap();
    let codex = dir.path().join("codex-login.json");
    fs::write(&codex, r#"{"auth_mode":"chatgpt","tokens":{"account_id":"fixture-codex","access_token":"fixture-token"}}"#).unwrap();
    let config = dir.path().join("config.json");
    fs::write(&config, serde_json::to_vec(&json!({"schemaVersion":2,"privateDirectory":dir.path().join("private"),
        "endpoints":{"codex":{"program":node,"script":script,"loginFile":codex},"codebuddy":{"program":node,"script":script,"loginFile":buddy}},
        "initialSessions":{"codex":["main-codex"],"codebuddy":["main-buddy"]}})).unwrap()).unwrap();
    let product = Collaboration::from_file(&config).unwrap();
    (dir, product, config)
}
fn call(
    product: &Collaboration,
    source: &str,
    session: &str,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    product.request(
        "host_tool",
        json!({"name":name,"caller":{"client":source,"sessionId":session},"arguments":args}),
    )
}
fn submit(product: &Collaboration, source: &str, session: &str, id: &str) -> Result<Value, String> {
    call(
        product,
        source,
        session,
        "collaboration_submit",
        json!({"taskId":id,"target":if source=="codex"{"codebuddy"}else{"codex"},"prompt":"fixture-only text"}),
    )
}

#[test]
fn both_directions_execute_and_return_the_bound_receipt() {
    let (_dir, p, _) = fixture();
    for (source, session, id) in [
        ("codex", "main-codex", "forward"),
        ("codebuddy", "main-buddy", "reverse"),
    ] {
        assert_eq!(submit(&p, source, session, id).unwrap()["state"], "queued");
        p.execute(id).unwrap();
        let result = call(
            &p,
            source,
            session,
            "collaboration_status",
            json!({"taskId":id}),
        )
        .unwrap();
        assert_eq!(result["state"], "completed");
        assert_eq!(result["result"], "fixture result");
        assert_eq!(submit(&p, source, session, id).unwrap(), result);
        p.execute(id).unwrap(); // A second drain cannot create a new process.
        assert_eq!(p.task(id).unwrap(), result);
    }
    assert_eq!(
        p.task("reverse").unwrap()["helperSessionId"],
        "fixture-codex-thread"
    );
}

#[test]
fn ownership_dedup_nested_and_foreign_task_access_are_rejected() {
    let (_dir, p, _) = fixture();
    submit(&p, "codex", "main-codex", "one").unwrap();
    assert!(submit(&p, "codebuddy", "main-buddy", "one").is_err());
    assert!(submit(&p, "codex", "other", "two").is_err());
    assert!(submit(&p, "codex", "main-codex", "../escape").is_err());
    assert!(call(
        &p,
        "codebuddy",
        "main-buddy",
        "collaboration_status",
        json!({"taskId":"one"})
    )
    .is_err());
    assert!(call(
        &p,
        "codex",
        "main-codex",
        "collaboration_submit",
        json!({"taskId":"one","target":"codebuddy","prompt":"changed"})
    )
    .is_err());
    assert!(call(
        &p,
        "codex",
        "main-codex",
        "collaboration_submit",
        json!({"taskId":"two","target":"codex","prompt":"text"})
    )
    .is_err());
    assert!(call(
        &p,
        "codex",
        "main-codex",
        "collaboration_submit",
        json!({"taskId":"two","target":"codebuddy","prompt":"text","depth":1})
    )
    .is_err());
    assert_eq!(
        call(
            &p,
            "codebuddy",
            "main-buddy",
            "collaboration_list",
            json!({})
        )
        .unwrap()["tasks"],
        json!([])
    );
    assert!(p
        .bind(&json!({"client":"codebuddy","sessionId":"ah-helper-one","enabled":true}))
        .is_err());
}

#[test]
fn cancel_restart_revoke_and_failed_spawn_do_not_replay() {
    let (dir, p, config) = fixture();
    submit(&p, "codex", "main-codex", "cancel").unwrap();
    p.cancel("cancel").unwrap();
    p.execute("cancel").unwrap();
    assert_eq!(p.task("cancel").unwrap()["state"], "cancelled");
    assert!(!p.config.private_directory.join("cancel").exists());
    submit(&p, "codex", "main-codex", "unfinished").unwrap();
    let reopened = Collaboration::from_file(&config).unwrap();
    assert_eq!(
        submit(&reopened, "codex", "main-codex", "unfinished").unwrap()["state"],
        "deliveryUnknown"
    );
    reopened
        .bind(&json!({"client":"codebuddy","sessionId":"main-buddy","enabled":false}))
        .unwrap();
    let reopened = Collaboration::from_file(&config).unwrap();
    assert!(submit(&reopened, "codebuddy", "main-buddy", "revoked").is_err());
    // A disappearing login fails BEFORE spawn.
    submit(&reopened, "codex", "main-codex", "changed").unwrap();
    fs::remove_file(dir.path().join("buddy-login.json")).unwrap();
    reopened.execute("changed").unwrap();
    assert_eq!(reopened.task("changed").unwrap()["state"], "failed");
    assert!(!reopened.config.private_directory.join("changed").exists());
}

#[test]
fn token_refresh_keeps_identity_but_account_switch_and_logout_block() {
    let (dir, p, _) = fixture();
    let before = p.scope("codebuddy").unwrap();
    let file = dir.path().join("buddy-login.json");
    fs::write(
        &file,
        r#"{"account":{"uid":"fixture-buddy"},"auth":{"accessToken":"rotated-token"}}"#,
    )
    .unwrap();
    assert_eq!(before, p.scope("codebuddy").unwrap());
    fs::write(
        &file,
        r#"{"account":{"uid":"another"},"auth":{"accessToken":"rotated-token"}}"#,
    )
    .unwrap();
    assert!(submit(&p, "codebuddy", "main-buddy", "switch").is_err());
    fs::write(dir.path().join("buddy-login.json.logged-out"), "logged out").unwrap();
    assert!(p.scope("codebuddy").is_err());
}

#[test]
fn failed_ledger_write_cannot_queue_unrecorded_work() {
    let (_dir, p, _) = fixture();
    fs::create_dir(p.config.private_directory.join("tasks-v2.pending.json")).unwrap();
    assert!(submit(&p, "codex", "main-codex", "unpersisted").is_err());
    assert!(p.task("unpersisted").is_err());
    assert!(p.faulted.load(Ordering::Acquire));
}

#[test]
fn terminal_parsers_reject_wrong_identity_extra_turn_and_tool_execution() {
    let valid = br#"{"type":"result","subtype":"success","session_id":"helper","result":"42"}"#;
    assert_eq!(
        adapter::terminal_result("codebuddy", "helper", valid).state,
        "completed"
    );
    assert_eq!(
        adapter::terminal_result("codebuddy", "wrong", valid).state,
        "deliveryUnknown"
    );
    let events = [
        json!({"type":"thread.started","thread_id":"thread"}),
        json!({"type":"turn.started"}),
        json!({"type":"item.completed","item":{"type":"agent_message","text":"42"}}),
        json!({"type":"turn.completed"}),
    ];
    let text = events
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        adapter::terminal_result("codex", "unused", text.as_bytes()).state,
        "completed"
    );
    assert_eq!(
        adapter::terminal_result(
            "codex",
            "unused",
            format!("{text}\n{{\"type\":\"turn.started\"}}").as_bytes()
        )
        .state,
        "deliveryUnknown"
    );
    assert_eq!(
        adapter::terminal_result(
            "codex",
            "unused",
            text.replace("agent_message", "command_execution")
                .as_bytes()
        )
        .state,
        "deliveryUnknown"
    );
}

#[test]
fn codebuddy_transcript_array_requires_one_matching_text_only_result() {
    let messages = json!([
        {"type":"message","role":"user","sessionId":"helper","content":[{"type":"input_text","text":"17+25"}]},
        {"type":"message","role":"assistant","sessionId":"helper","content":[{"type":"output_text","text":"42"}]},
        {"type":"result","subtype":"success","is_error":false,"session_id":"helper","num_turns":2,"result":"42"}
    ]);
    let parse =
        |v: &Value| adapter::terminal_result("codebuddy", "helper", v.to_string().as_bytes());
    assert_eq!(parse(&messages).state, "completed");
    assert_eq!(parse(&messages).message, "42");
    let mut bad = messages.clone();
    bad[0]["sessionId"] = json!("foreign");
    assert_eq!(parse(&bad).state, "deliveryUnknown");
    let mut bad = messages.clone();
    bad[1]["content"][0]["type"] = json!("tool_use");
    assert_eq!(parse(&bad).state, "deliveryUnknown");
    let mut bad = messages.clone();
    bad[2]["num_turns"] = json!(3);
    assert_eq!(parse(&bad).state, "deliveryUnknown");
    let mut bad = messages.clone();
    bad.as_array_mut().unwrap().push(messages[2].clone());
    assert_eq!(parse(&bad).state, "deliveryUnknown");
}

#[test]
fn cancellation_stops_a_running_fixture_and_retains_unknown_without_replay() {
    let (dir, p, _) = fixture();
    fs::write(dir.path().join("fixture.cjs"), "setInterval(()=>{},1000);").unwrap();
    submit(&p, "codex", "main-codex", "running").unwrap();
    let worker = Arc::clone(&p);
    let handle = std::thread::spawn(move || worker.execute("running"));
    for _ in 0..100 {
        if p.config
            .private_directory
            .join("running/stdout.json")
            .exists()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Allow the fixture to reach spawn before requesting local cancellation.
    std::thread::sleep(Duration::from_millis(150));
    p.cancel("running").unwrap();
    handle.join().unwrap().unwrap();
    assert_eq!(p.task("running").unwrap()["state"], "deliveryUnknown");
    assert_eq!(
        submit(&p, "codex", "main-codex", "running").unwrap()["state"],
        "deliveryUnknown"
    );
}

#[test]
fn commands_keep_task_text_as_one_argument_and_disable_recursive_delegation() {
    let (_dir, p, _) = fixture();
    let task = json!({"prompt":"text ; $(do-not-run)","helperSessionId":"ah-helper-test"});
    for source in ["codex", "codebuddy"] {
        let command = adapter::command(
            source,
            &p.config.endpoints[source],
            &p.config.private_directory,
            &task,
        );
        let args = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args.last().unwrap(), "text ; $(do-not-run)");
        assert!(command
            .get_envs()
            .any(|(k, v)| k == "AGENTHUD_HELPER" && v == Some(std::ffi::OsStr::new("1"))));
        assert!(!args.iter().any(|s| s.contains("bypass")));
        if source == "codebuddy" {
            assert!(args.windows(2).any(|w| w == ["--setting-sources", "none"]));
        } else {
            assert!(args.windows(2).any(|w| w == ["--sandbox", "read-only"]));
            assert!(!args
                .iter()
                .any(|s| s.starts_with("model_providers.openai.")));
        }
    }
}
