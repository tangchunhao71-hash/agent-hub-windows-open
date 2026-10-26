// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
use super::*;
fn snapshot(items: Vec<Value>, account: &str) -> Value {
    json!({"mode":"live","providers":[{"id":"codex","detected":true}],"accounts":[{"provider":"codex","id":account}],
        "islandEvents":{"items":items},"sessions":[]})
}
fn event(turn: &str, at: i64) -> Value {
    json!({"id":format!("ephemeral-{turn}"),"kind":"completion","source":"codex","sessionId":"session-a","turnId":turn,
        "at":chrono::DateTime::from_timestamp_millis(at).unwrap().to_rfc3339(),"title":"Private task title",
        "message":"Private reply body","botIdentity":{"name":"Private Name","avatar":{"dataUrl":"secret-photo"}},
        "navigationTarget":{"kind":"codex-thread","id":"original-thread","extra":"secret"}})
}
fn sync(owner: &str) -> Value {
    json!({"owner":owner,"eligible":true})
}
#[test]
fn deepseek_completion_keeps_only_its_identifier_navigation() {
    assert_eq!(
        safe_navigation(
            &json!({"kind":"deepseek-session","id":"sess_abc","url":"private","path":"private"})
        ),
        Some(json!({"kind":"deepseek-session","id":"sess_abc"}))
    );
}
fn grok_snapshot(mut items: Vec<Value>, account: &str) -> Value {
    let slot = format!("{:x}", Sha256::digest(account.as_bytes()));
    for event in &mut items {
        event["source"] = json!("grok-bot");
        event["sessionId"] = json!(format!("grok-bot:{slot}:bot-a"));
    }
    let mut value = snapshot(items, account);
    value["accounts"][0]["id"] = json!(grok_account(&slot));
    value["accounts"][0]["provider"] = json!("grok-bot");
    value["providers"][0]["id"] = json!("grok-bot");
    value
}
#[test]
fn first_projected_snapshot_delivers_trusted_completion_once_and_ack_survives_restart() {
    let folder = tempfile::tempdir().unwrap();
    let owner = Presentation::new(folder.path().into()).unwrap();
    let at = super::super::now();
    // Core emits only completions after its own startup. Its first published
    // snapshot may already contain a real completion from this process.
    let mut first = snapshot(vec![event("first-frame", at)], "a");
    owner.project_snapshot(&mut first);
    let delivered = owner.completion_inbox(sync("screen")).unwrap();
    assert_eq!(
        delivered["items"].as_array().unwrap().len(),
        1,
        "first-frame Core completion must remain unread"
    );
    let id = delivered["items"][0]["id"].clone();
    owner.project_snapshot(&mut first);
    let repeated = owner.completion_inbox(sync("screen")).unwrap();
    assert_eq!(repeated["items"].as_array().unwrap().len(), 1);
    assert_eq!(repeated["items"][0]["id"], id);
    let bytes = fs::read_to_string(folder.path().join("completion-inbox.json")).unwrap();
    for secret in ["Private", "secret-photo", "extra", "message", "botIdentity"] {
        assert!(!bytes.contains(secret));
    }
    owner
        .completion_inbox(json!({"owner":"screen","operation":"dismiss","ids":[id]}))
        .unwrap();
    drop(owner);
    let reopened = Presentation::new(folder.path().into()).unwrap();
    reopened.project_snapshot(&mut first);
    assert!(reopened.completion_inbox(sync("other")).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn first_projected_snapshot_while_disabled_is_consumed_without_replay_when_enabled() {
    let folder = tempfile::tempdir().unwrap();
    let owner = Presentation::new(folder.path().into()).unwrap();
    owner
        .update(json!({"screen":"all","patch":{"completionNotifications":false}}))
        .unwrap();
    let at = super::super::now();
    let mut first = snapshot(vec![event("off-first-frame", at)], "a");
    owner.project_snapshot(&mut first);
    assert!(owner.completion_inbox(sync("screen")).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    owner
        .update(json!({"screen":"all","patch":{"completionNotifications":true}}))
        .unwrap();
    owner.project_snapshot(&mut first);
    assert!(owner.completion_inbox(sync("screen")).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    first["islandEvents"]["items"]
        .as_array_mut()
        .unwrap()
        .push(event("after-enable", at));
    owner.project_snapshot(&mut first);
    let delivered = owner.completion_inbox(sync("screen")).unwrap();
    assert_eq!(delivered["items"].as_array().unwrap().len(), 1);
    assert_eq!(delivered["items"][0]["turnId"], "after-enable");
}
#[test]
fn restart_restores_only_unread_without_private_payload_and_keeps_original_target() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    // History before Core startup is filtered in Core, not simulated as a new
    // completion here. This genuine event was consumed while delivery was off.
    inbox.observe(
        &snapshot(vec![event("off", now - 100)], "a"),
        false,
        &Map::new(),
        now,
    );
    assert!(inbox.request(sync("left"), now).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    inbox.observe(
        &snapshot(vec![event("new", now)], "a"),
        true,
        &Map::new(),
        now,
    );
    let first = inbox.request(sync("left"), now).unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let bytes = fs::read_to_string(folder.path().join("completion-inbox.json")).unwrap();
    for secret in ["Private", "secret-photo", "extra", "message", "botIdentity"] {
        assert!(!bytes.contains(secret));
    }
    drop(inbox);
    let mut reopened = Inbox::new(folder.path().into());
    let mut sample = snapshot(vec![], "a");
    sample["sessions"] = json!([{"id":"session-a","source":"codex","navigationTarget":{"kind":"codex-thread","id":"newer-thread"}}]);
    reopened.observe(&sample, true, &Map::new(), now);
    let restored = reopened.request(sync("right"), now).unwrap();
    assert_eq!(restored["items"][0]["id"], first["items"][0]["id"]);
    assert_eq!(
        restored["items"][0]["navigationTarget"]["id"],
        "original-thread"
    );
    let id = restored["items"][0]["id"].clone();
    reopened
        .request(
            json!({"owner":"right","operation":"dismiss","ids":[id]}),
            now,
        )
        .unwrap();
    let mut again = Inbox::new(folder.path().into());
    again.observe(
        &snapshot(vec![event("new", now)], "a"),
        true,
        &Map::new(),
        now,
    );
    assert!(again.request(sync("last"), now).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn one_screen_owns_the_queue_and_a_closed_or_crashed_owner_can_be_replaced() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    inbox.observe(&snapshot(vec![], "a"), true, &Map::new(), now);
    inbox.observe(
        &snapshot(vec![event("one", now)], "a"),
        true,
        &Map::new(),
        now,
    );
    assert_eq!(
        inbox.request(sync("left"), now).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(inbox.request(sync("right"), now + 1).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(inbox
        .request(
            json!({"owner":"right","operation":"dismiss","ids":[]}),
            now + 1
        )
        .is_err());
    assert_eq!(
        inbox.request(sync("right"), now + LEASE_MS + 1).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    inbox
        .request(
            json!({"owner":"left","operation":"release"}),
            now + LEASE_MS + 2,
        )
        .unwrap();
    assert_eq!(
        inbox.request(sync("right"), now + LEASE_MS + 3).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    inbox
        .request(
            json!({"owner":"right","operation":"release"}),
            now + LEASE_MS + 4,
        )
        .unwrap();
    assert_eq!(
        inbox.request(sync("left"), now + LEASE_MS + 5).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn account_switch_hides_old_records_and_restoring_account_reveals_them_without_reinserting() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    inbox.observe(&grok_snapshot(vec![], "a"), true, &Map::new(), now);
    let original = grok_snapshot(vec![event("one", now)], "a");
    inbox.observe(&original, true, &Map::new(), now);
    let old = inbox.request(sync("screen"), now).unwrap()["items"][0]["id"].clone();
    let mut switched = grok_snapshot(vec![], "b");
    switched["islandEvents"] = original["islandEvents"].clone();
    inbox.observe(&switched, true, &Map::new(), now);
    assert!(inbox.request(sync("screen"), now).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(
        inbox
            .request(
                json!({"owner":"screen","operation":"dismiss","ids":[old]}),
                now
            )
            .is_err(),
        "late ack cannot delete another account's hidden unread"
    );
    assert_eq!(inbox.saved.unread.len(), 1);
    inbox.observe(&grok_snapshot(vec![], "a"), true, &Map::new(), now);
    assert_eq!(
        inbox.request(sync("screen"), now).unwrap()["items"][0]["id"],
        old
    );
    assert_eq!(inbox.saved.unread.len(), 1);
    let mut grok = event("grok", now);
    grok["source"] = json!("grok-bot");
    assert!(record(&snapshot(vec![], "a"), &grok, now).is_none());
}
#[test]
fn failed_ack_keeps_the_card_and_old_async_ids_cannot_delete_a_new_reply() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    inbox.observe(&snapshot(vec![], "a"), true, &Map::new(), now);
    inbox.observe(
        &snapshot(vec![event("one", now)], "a"),
        true,
        &Map::new(),
        now,
    );
    let first = inbox.request(sync("screen"), now).unwrap()["items"][0]["id"].clone();
    let tmp = folder.path().join("completion-inbox.json.tmp");
    fs::create_dir(&tmp).unwrap();
    assert!(inbox
        .request(
            json!({"owner":"screen","operation":"dismiss","ids":[first]}),
            now
        )
        .is_err());
    assert_eq!(inbox.saved.unread.len(), 1);
    fs::remove_dir(&tmp).unwrap();
    inbox.observe(
        &snapshot(vec![event("two", now + 1)], "a"),
        true,
        &Map::new(),
        now + 1,
    );
    let view = inbox
        .request(
            json!({"owner":"screen","operation":"dismiss","ids":[first]}),
            now + 1,
        )
        .unwrap();
    assert_eq!(view["items"].as_array().unwrap().len(), 1);
    assert_eq!(view["items"][0]["turnId"], "two");
}
#[test]
fn disabled_delivery_consumes_events_and_retention_is_bounded() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    inbox.observe(
        &snapshot(vec![event("off", now)], "a"),
        false,
        &Map::new(),
        now,
    );
    inbox.observe(
        &snapshot(vec![event("off", now)], "a"),
        true,
        &Map::new(),
        now,
    );
    assert!(inbox.request(sync("screen"), now).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    let rows = (0..300).map(|i| event(&i.to_string(), now)).collect();
    inbox.observe(&snapshot(rows, "a"), true, &Map::new(), now);
    let view = inbox.request(sync("screen"), now).unwrap();
    assert_eq!(view["items"].as_array().unwrap().len(), MAX_UNREAD);
    assert_eq!(view["dropped"], 44);
    inbox.observe(&snapshot(vec![], "a"), true, &Map::new(), now + KEEP_MS + 1);
    assert!(inbox.saved.unread.is_empty());
    assert_eq!(inbox.saved.dropped, 0);
}
#[test]
fn failed_ingest_is_retried_without_another_completion_and_corrupt_files_are_preserved() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    inbox.observe(&snapshot(vec![], "a"), true, &Map::new(), now);
    let temporary = folder.path().join("completion-inbox.json.tmp");
    fs::create_dir(&temporary).unwrap();
    inbox.observe(
        &snapshot(vec![event("one", now)], "a"),
        true,
        &Map::new(),
        now,
    );
    assert!(inbox.request(sync("screen"), now).unwrap()["error"].is_string());
    fs::remove_dir(temporary).unwrap();
    assert!(inbox.request(sync("screen"), now).unwrap()["error"].is_null());
    drop(inbox);
    let mut reopened = Inbox::new(folder.path().into());
    reopened.observe(&snapshot(vec![], "a"), true, &Map::new(), now);
    assert_eq!(
        reopened.request(sync("other"), now).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let path = folder.path().join("completion-inbox.json");
    fs::write(&path, "bad original bytes").unwrap();
    let mut broken = Inbox::new(folder.path().into());
    broken.observe(
        &snapshot(vec![event("new", now)], "a"),
        true,
        &Map::new(),
        now,
    );
    assert!(broken.request(sync("third"), now).unwrap()["error"].is_string());
    assert_eq!(fs::read_to_string(path).unwrap(), "bad original bytes");
}
#[test]
fn presentation_owner_serializes_concurrent_screen_claims_and_migrates_combined_preference() {
    use std::sync::Arc;
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let owner = Arc::new(Presentation::new(folder.path().into()).unwrap());
    let mut first = snapshot(vec![], "a");
    owner.project_snapshot(&mut first);
    let mut second = snapshot(vec![event("one", now)], "a");
    owner.project_snapshot(&mut second);
    let workers = ["left", "right"].map(|name| {
        let owner = Arc::clone(&owner);
        std::thread::spawn(move || {
            owner.completion_inbox(sync(name)).unwrap()["items"]
                .as_array()
                .unwrap()
                .len()
        })
    });
    assert_eq!(
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .sum::<usize>(),
        1
    );
    let legacy = tempfile::tempdir().unwrap();
    fs::write(
        legacy.path().join("presentation.json"),
        r#"{"version":3,"global":{"completionDrops":false},"screens":{}}"#,
    )
    .unwrap();
    let migrated = Presentation::new(legacy.path().into()).unwrap();
    assert_eq!(
        migrated.view()["store"]["global"]["completionNotifications"],
        false
    );
    migrated
        .update(json!({"screen":"all","patch":{"completionDrops":true}}))
        .unwrap();
    assert_eq!(
        migrated.view()["store"]["global"]["completionNotifications"],
        false
    );
    migrated
        .update(json!({"screen":"all","patch":{"completionNotifications":true}}))
        .unwrap();
    assert_eq!(
        Presentation::new(legacy.path().into()).unwrap().view()["store"]["global"]
            ["completionNotifications"],
        true
    );
}
#[test]
fn a_large_history_keeps_bounded_tombstones_and_never_replays_the_current_event_window() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    inbox.observe(&snapshot(vec![], "a"), true, &Map::new(), now);
    let rows = (0..2200)
        .map(|i| event(&i.to_string(), now))
        .collect::<Vec<_>>();
    inbox.observe(&snapshot(rows.clone(), "a"), true, &Map::new(), now);
    assert_eq!(inbox.saved.seen.len(), MAX_SEEN);
    assert_eq!(inbox.saved.unread.len(), MAX_UNREAD);
    let all = inbox.request(sync("screen"), now).unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].clone())
        .collect::<Vec<_>>();
    inbox
        .request(
            json!({"owner":"screen","operation":"dismiss","ids":all}),
            now,
        )
        .unwrap();
    inbox.observe(
        &snapshot(rows.into_iter().skip(2168).collect(), "a"),
        true,
        &Map::new(),
        now,
    );
    assert!(inbox.request(sync("screen"), now).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn quota_account_loading_cannot_change_local_session_identity_and_active_screens_handoff() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    let mut empty = snapshot(vec![], "a");
    empty["accounts"] = json!([]);
    inbox.observe(&empty, true, &Map::new(), now);
    empty["islandEvents"]["items"] = json!([event("one", now)]);
    inbox.observe(&empty, true, &Map::new(), now);
    let first = inbox.request(sync("left"), now).unwrap()["items"][0]["id"].clone();
    inbox.observe(&snapshot(vec![], "a"), true, &Map::new(), now);
    assert_eq!(
        inbox.request(sync("left"), now).unwrap()["items"][0]["id"],
        first
    );
    inbox
        .request(json!({"owner":"left","eligible":false}), now)
        .unwrap();
    assert_eq!(
        inbox.request(sync("right"), now).unwrap()["items"][0]["id"],
        first
    );
    assert!(inbox
        .request(json!({"owner":"left","eligible":false}), now)
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    inbox
        .request(json!({"owner":"right","eligible":false}), now)
        .unwrap();
    assert_eq!(
        inbox.request(sync("left"), now).unwrap()["items"][0]["id"],
        first
    );
}
#[test]
fn unknown_grok_login_cannot_rebind_an_old_slots_event_when_another_account_hydrates() {
    let folder = tempfile::tempdir().unwrap();
    let now = super::super::now();
    let mut inbox = Inbox::new(folder.path().into());
    let mut unknown = grok_snapshot(vec![], "a");
    unknown["accounts"] = json!([]);
    inbox.observe(&unknown, true, &Map::new(), now);
    unknown["islandEvents"] = grok_snapshot(vec![event("one", now)], "a")["islandEvents"].clone();
    inbox.observe(&unknown, true, &Map::new(), now);
    assert!(inbox.saved.unread.is_empty());
    let mut different = grok_snapshot(vec![], "b");
    different["islandEvents"] = unknown["islandEvents"].clone();
    inbox.observe(&different, true, &Map::new(), now);
    assert!(inbox.saved.unread.is_empty());
    let mut proper = grok_snapshot(vec![], "a");
    proper["islandEvents"] = unknown["islandEvents"].clone();
    inbox.observe(&proper, true, &Map::new(), now);
    assert_eq!(
        inbox.request(sync("screen"), now).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
