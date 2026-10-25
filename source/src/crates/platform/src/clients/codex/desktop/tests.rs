use super::*;
use serde_json::json;

const THREAD: &str = "00000000-0000-4000-8000-000000000001";
fn permission(connector: &str, persist: Value) -> Value {
    json!({"id":7,"method":"mcpServer/elicitation/request","params":{"threadId":THREAD,"turnId":"turn-fixture","mode":"form","_meta":{"codex_approval_kind":"mcp_tool_call","connector_id":connector,"persist":persist,"tool_params":{"app":"com.example.Fixture","origin":"https://fixture.invalid"},"tool_name":"access_browser_origin"}}})
}
fn snapshot(requests: Vec<Value>) -> Value {
    json!({"cwd":"/fixture","requests":requests,"currentPermissions":{"approvalsReviewer":"user","approvalPolicy":"on-request","sandboxPolicy":{"type":"workspaceWrite"}},"turns":[]})
}

fn command_permission() -> Value {
    json!({"id":"command-fixture","method":"item/commandExecution/requestApproval",
        "params":{"threadId":THREAD,"turnId":"turn-fixture","itemId":"command-item",
            "command":"Get-Content -LiteralPath C:/fixture/approval.txt","cwd":"C:/fixture",
            "reason":"Read only the dedicated test file"}})
}
fn command_snapshot() -> Value {
    let mut state = snapshot(vec![command_permission()]);
    state["turns"] = json!([{"turnId":"turn-fixture","status":"inProgress",
        "params":{"approvalsReviewer":"user","approvalPolicy":"on-request"}}]);
    state
}

fn file_permissions_snapshot() -> Value {
    let mut state = command_snapshot();
    let path = std::env::temp_dir().join("agent-hud-approval-fixture.txt");
    state["turns"][0]["params"]["approvalPolicy"] = json!({"granular":{"sandbox_approval":false,"request_permissions":true}});
    state["requests"] = json!([{"id":"files-fixture","method":"item/permissions/requestApproval",
        "params":{"threadId":THREAD,"turnId":"turn-fixture","reason":"Read one test file",
            "permissions":{"fileSystem":{"entries":[{"path":{"type":"path","path":path},"access":"read"}]}}}}]);
    state
}

#[test]
fn file_permissions_display_exact_requested_paths_and_grant_only_this_turn() {
    let mut state = file_permissions_snapshot();
    let second = std::env::temp_dir().join("agent-hud-write-fixture.txt");
    state["requests"][0]["params"]["permissions"]["fileSystem"]["entries"].as_array_mut().unwrap()
        .push(json!({"access":"write","path":{"type":"path","path":second}}));
    let requested = &state["requests"][0]["params"]["permissions"];
    let found = pending::parse(&state, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    let expected = requested["fileSystem"]["entries"].as_array().unwrap().iter().map(|entry| {
        let access = if entry["access"] == "read" {agent_hud_core::l10n::text("读取", "Read")} else {agent_hud_core::l10n::text("写入", "Write")};
        format!("{access}: {}", entry["path"]["path"].as_str().unwrap())
    }).collect::<Vec<_>>().join("\n");
    assert_eq!(found[0].view["input"], expected, "every read/write path must be visible");
    assert_eq!(found[0].view["badge"], "FILES");
    assert_eq!(found[0].view["allowsAlways"], false);
    for (kind, grants) in [("allow", requested.clone()), ("deny", json!({}))] {
        let (method, response) = pending::answer(&found[0], &json!({"kind":kind}), &state).unwrap();
        assert_eq!(method, "thread-follower-permissions-request-approval-response");
        assert_eq!(response, json!({"permissions":grants,"scope":"turn"}));
    }
    for kind in ["always", "session", "answer"] {
        assert!(pending::answer(&found[0], &json!({"kind":kind}), &state).is_err());
    }
    let mut withdrawn = state.clone();
    withdrawn["requests"] = json!([]);
    assert!(pending::answer(&found[0], &json!({"kind":"allow"}), &withdrawn).is_err());
}

#[test]
fn file_permissions_reject_unsupported_ranges_unknown_fields_and_stale_identity() {
    let original = file_permissions_snapshot();
    for (pointer, value) in [
        ("/requests/0/params/threadId", json!("another-thread")),
        ("/requests/0/params/turnId", json!("old-turn")),
        ("/requests/0/id", Value::Null),
        ("/turns/0/status", json!("completed")),
        ("/turns/0/params/approvalsReviewer", json!("auto")),
        ("/turns/0/params/approvalPolicy", json!({"granular":{"request_permissions":false}})),
        ("/turns/0/params/approvalPolicy", json!("never")),
        ("/turns/0/params/approvalPolicy", json!("unknown")),
        ("/requests/0/params/permissions", json!({"network":{"enabled":true},"fileSystem":{"entries":[]}})),
        ("/requests/0/params/permissions", json!({"unknown":{"grant":true}})),
        ("/requests/0/params/permissions/fileSystem", json!({"entries":[],"unknown":true})),
        ("/requests/0/params/permissions/fileSystem/entries", json!([])),
        ("/requests/0/params/permissions/fileSystem/entries/0/access", json!("execute")),
        ("/requests/0/params/permissions/fileSystem/entries/0/path", json!({"type":"special","value":{"kind":"root"}})),
        ("/requests/0/params/permissions/fileSystem/entries/0/path", json!({"type":"glob_pattern","pattern":"**/*"})),
        ("/requests/0/params/permissions/fileSystem/entries/0/path", json!({"type":"path","path":"relative.txt"})),
        ("/requests/0/params/permissions/fileSystem/entries/0/path", json!({"type":"path","path":"/tmp/file\nunsafe"})),
        ("/requests/0/params/permissions/fileSystem/entries/0/path", json!({"type":"path","path":"/tmp/fixture","unknown":true})),
    ] {
        let mut state = original.clone();
        *state.pointer_mut(pointer).unwrap() = value;
        assert!(pending::parse(&state, THREAD, "owner").unwrap().is_empty(), "unsupported request must not be approvable: {state}");
    }
    let mut completed = original.clone();
    completed["requests"][0]["completed"] = json!(true);
    assert!(pending::parse(&completed, THREAD, "owner").unwrap().is_empty());
    let mut spaced = original.clone();
    spaced["requests"][0]["params"]["permissions"]["fileSystem"]["entries"][0]["path"]["path"] =
        json!(format!(" {}", std::env::temp_dir().join("fixture.txt").display()));
    assert!(pending::parse(&spaced, THREAD, "owner").unwrap().is_empty(), "do not validate a trimmed path while granting different original bytes");
}

#[test]
fn command_approval_is_single_use_and_bound_to_the_live_turn() {
    let mut state = command_snapshot();
    state["requests"][0]["params"]["command"] = json!("\nGet-Content -LiteralPath C:/fixture/approval.txt\n");
    let found = pending::parse(&state, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].view["sessionId"], THREAD);
    assert_eq!(found[0].view["badge"], "CMD");
    assert_eq!(found[0].view["input"], state["requests"][0]["params"]["command"]);
    assert_eq!(found[0].view["allowsAlways"], false);
    for (kind, expected) in [("allow", "accept"), ("deny", "decline")] {
        let (method, answer) = pending::answer(&found[0], &json!({"kind":kind}), &state).unwrap();
        assert_eq!(method, "thread-follower-command-approval-decision");
        assert_eq!(answer, expected);
    }
    assert!(pending::answer(&found[0], &json!({"kind":"always"}), &state).is_err());
    assert!(pending::answer(&found[0], &json!({"kind":"answer","answers":{}}), &state).is_err());
    let mut changed = state.clone();
    changed["turns"][0]["turnId"] = json!("new-turn");
    assert!(pending::parse(&changed, THREAD, "owner").unwrap().is_empty());
    assert!(pending::answer(&found[0], &json!({"kind":"allow"}), &changed).is_err());
}

#[test]
fn command_approval_rejects_unsupported_stale_or_foreign_requests() {
    let original = command_snapshot();
    for (path, value) in [
        (vec!["requests","0","completed"], json!(true)),
        (vec!["requests","0","params","threadId"], json!("another-thread")),
        (vec!["requests","0","params","turnId"], json!("old-turn")),
        (vec!["requests","0","params","itemId"], Value::Null),
        (vec!["requests","0","params","command"], Value::Null),
        (vec!["requests","0","params","command"], json!("")),
        (vec!["requests","0","id"], Value::Null),
        (vec!["turns","0","status"], json!("completed")),
        (vec!["turns","0","params","approvalsReviewer"], json!("auto")),
        (vec!["turns","0","params","approvalPolicy"], json!("never")),
        (vec!["turns","0","params","approvalPolicy"], json!({"granular":{"sandbox_approval":false}})),
        (vec!["turns","0","params","approvalPolicy"], json!("unknown")),
        (vec!["requests","0","method"], json!("execCommandApproval")),
        (vec!["requests","0","method"], json!("item/fileChange/requestApproval")),
    ] {
        let mut state = original.clone();
        let mut selected = &mut state;
        for component in path {
            selected = if let Ok(index) = component.parse::<usize>() { &mut selected[index] } else { &mut selected[component] };
        }
        *selected = value;
        assert!(pending::parse(&state, THREAD, "owner").unwrap().is_empty(), "request must not be offered: {state}");
    }
}

#[test]
fn scopes_and_browser_reviewer_match_source() {
    let state = snapshot(vec![permission(
        "computer-use",
        json!(["session", "always"]),
    )]);
    let found = pending::parse(&state, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].view["allowsAlways"], true);
    assert_eq!(
        pending::answer(&found[0], &json!({"kind":"allow"}), &state)
            .unwrap()
            .1["_meta"]["persist"],
        "session"
    );
    assert_eq!(
        pending::answer(&found[0], &json!({"kind":"always"}), &state)
            .unwrap()
            .1["_meta"]["persist"],
        "always"
    );
    assert_eq!(
        pending::answer(&found[0], &json!({"kind":"deny"}), &state)
            .unwrap()
            .1,
        json!({"action":"decline","content":null,"_meta":null})
    );
    assert!(pending::parse(
        &snapshot(vec![permission("computer-use", json!(["unknown"]))]),
        THREAD,
        "owner"
    )
    .unwrap()
    .is_empty());
    let mut browser = snapshot(vec![permission("browser-use", Value::Null)]);
    let found = pending::parse(&browser, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    assert!(
        pending::answer(&found[0], &json!({"kind":"allow"}), &browser)
            .unwrap()
            .1["_meta"]
            .is_null()
    );
    assert!(pending::answer(&found[0], &json!({"kind":"always"}), &browser).is_err());
    browser["currentPermissions"]["approvalsReviewer"] = json!("auto");
    assert!(pending::parse(&browser, THREAD, "owner")
        .unwrap()
        .is_empty());
    browser["currentPermissions"] = json!({"approvalsReviewer":"user","approvalPolicy":"never","sandboxPolicy":{"type":"dangerFullAccess"}});
    assert!(pending::parse(&browser, THREAD, "owner")
        .unwrap()
        .is_empty());
}
#[test]
fn native_cards_say_what_allowing_keeps_as_open_words_it() {
    use agent_hud_core::l10n;
    let conversation = l10n::text("允许此对话", "Allow this conversation");
    let once = l10n::text("批准一次", "Allow once");
    let app = snapshot(vec![permission(
        "computer-use",
        json!(["session", "always"]),
    )]);
    let card = &pending::parse(&app, THREAD, "owner").unwrap()[0].view;
    assert!(
        card["alwaysAllowRule"].is_null(),
        "an app has no rule to show: {card}"
    );
    assert_eq!(card["allowLabel"], json!(conversation));
    assert_eq!(card["badge"], "APP");
    let site = snapshot(vec![permission(
        "browser-use",
        json!(["session", "always"]),
    )]);
    let card = &pending::parse(&site, THREAD, "owner").unwrap()[0].view;
    assert_eq!(
        card["alwaysAllowRule"], "https://fixture.invalid",
        "always allowing adds the site"
    );
    assert_eq!(card["allowLabel"], json!(conversation));
    assert_eq!(card["badge"], "WEB");
    let once_only = snapshot(vec![permission("browser-use", Value::Null)]);
    let card = &pending::parse(&once_only, THREAD, "owner").unwrap()[0].view;
    assert!(card["alwaysAllowRule"].is_null());
    assert_eq!(card["allowLabel"], json!(once));
    // A browser request whose turn carries no settings shows no card; only
    // one no turn is found for falls back on the current permissions.
    let mut unsettled = snapshot(vec![permission("browser-use", Value::Null)]);
    unsettled["turns"] = json!([{"turnId":"turn-fixture"}]);
    assert!(pending::parse(&unsettled, THREAD, "owner")
        .unwrap()
        .is_empty());
    unsettled["turns"] = json!([{"turnId":"turn-fixture","params":{"approvalsReviewer":"user"}}]);
    assert_eq!(
        pending::parse(&unsettled, THREAD, "owner").unwrap().len(),
        1,
        "the turn's own settings count"
    );
}
#[test]
fn canonical_async_question_answer_and_explicit_settlement() {
    let mut state = snapshot(vec![]);
    state["turnHistory"] = json!({"kind":"canonical","history":{"islands":[{"entries":[{"value":"key"}]}],"entitiesByKey":{"key":{"turnId":"turn-fixture","status":"inProgress","items":[{"type":"agentMessage","id":"message","questions":[{"title":"Pick one","options":["First","Second"]}]}]}}}});
    let found = pending::parse(&state, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    let key = found[0].request_id.as_str().unwrap();
    let (method, response) = pending::answer(
        &found[0],
        &json!({"kind":"answer","answers":{key:{"selected":["First"],"custom":"extra"}}}),
        &state,
    )
    .unwrap();
    assert_eq!(method, "thread-follower-steer-turn");
    assert!(response.as_str().unwrap().contains("First, extra"));
    assert!(pending::answer(
        &found[0],
        &json!({"kind":"answer","answers":{key:{"selected":["Invalid"]}}}),
        &state
    )
    .is_err());
    state["turnHistory"]["history"]["entitiesByKey"]["key"]["items"].as_array_mut().unwrap().push(json!({"type":"steeringUserMessage","status":"accepted","input":[{"type":"text","text":response}]}));
    assert!(pending::parse(&state, THREAD, "owner").unwrap().is_empty());
    state["turns"] = json!([{"turnId":"turn-fixture","status":"completed"}]);
    assert!(pending::parse(&state, THREAD, "owner").is_err()); // overlap cannot prove withdrawal
}
#[test]
fn immer_patch_operations_follow_source() {
    let mut state = json!({"items":[1,2,3],"nested":{"x":1}});
    stream::apply_patches(&json!([{"op":"replace","path":["items","length"],"value":2},{"op":"add","path":["items","-"],"value":4},{"op":"replace","path":["items",3],"value":5},{"op":"remove","path":["items",0]},{"op":"add","path":["nested","y"],"value":2}]),&mut state).unwrap();
    assert_eq!(state, json!({"items":[2,4,5],"nested":{"x":1,"y":2}}));
    stream::apply_patches(&json!([{"op":"nonsense","path":["missing"]},{"op":"replace","path":[],"value":{"ok":true}}]),&mut state).unwrap();
    assert_eq!(state, json!({"ok":true}));
    assert!(stream::apply_patches(
        &json!([{"op":"replace","path":["missing","child"],"value":1}]),
        &mut state
    )
    .is_err());
}

#[cfg(unix)]
#[path = "tests_unix.rs"]
mod unix;
#[cfg(windows)]
#[path = "tests_windows.rs"]
mod windows;
