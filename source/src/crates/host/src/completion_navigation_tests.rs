//! Exercise the same durable lookup -> Runtime navigation path the host invokes,
//! with a real reopened store and Runtime, without opening a user's client.
use agent_hud_hud::presentation::Presentation;
use agent_hud_runtime::Runtime;
use serde_json::json;

#[test]
fn unread_safe_navigation_survives_removal_from_core_and_requires_lease_and_account() {
    let folder=tempfile::tempdir().unwrap();
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let at=chrono::DateTime::from_timestamp_millis(now).unwrap().to_rfc3339();
    // Known Grok fixture slot hashes and their current account IDs, generated
    // by the source reader's legacy_unresolved algorithm (not quota guessing).
    let slot="a".repeat(64);
    let account_a="account:a220998a98c102dc1cc46d7b31cab77434d8a75f984a3c67bfbe54b1aca0a75a";
    let sample=|items:serde_json::Value,account:&str|json!({"mode":"demo","providers":[{"id":"grok-bot","detected":true}],
        "accounts":[{"provider":"grok-bot","id":if account=="a"{account_a}else{"different-account"}}],"sessions":[],"islandEvents":{"items":items}});
    let owner=Presentation::new(folder.path().join("presentation")).unwrap();
    owner.project_snapshot(&mut sample(json!([]),"a"));
    let session=format!("grok-bot:{slot}:removed-core-session");
    owner.project_snapshot(&mut sample(json!([{"kind":"completion","id":"ephemeral","source":"grok-bot","sessionId":session,"turnId":"settled-turn","at":at,
        "navigationTarget":{"kind":"grok-bot-agent","id":"33d873b3-7361-4b24-9577-f649b068f702"}}]),"a"));
    drop(owner);
    let owner=Presentation::new(folder.path().join("presentation")).unwrap();
    owner.project_snapshot(&mut sample(json!([]),"a"));
    let view=owner.completion_inbox(json!({"owner":"left","eligible":true})).unwrap();
    let id=view["items"][0]["id"].clone();
    let params=json!({"owner":"left","operation":"navigate","id":id});
    let target=owner.completion_navigation(&params).unwrap();
    let mut runtime=Runtime::demo(folder.path().join("runtime"),None).unwrap();
    assert!(runtime.request("session_detail",json!({"id":session})).is_err());
    assert_eq!(runtime.open_completion_target(&target).unwrap()["opened"],true);
    assert!(runtime.request("open_completion_target",json!({"target":target})).is_err(),"the Rust-only helper must not be exposed as an arbitrary native_request operation");
    assert_eq!(owner.completion_inbox(json!({"owner":"left","eligible":true})).unwrap()["items"].as_array().unwrap().len(),1,"navigation is not acknowledgement");
    assert!(owner.completion_navigation(&json!({"owner":"right","operation":"navigate","id":id})).is_err());
    assert!(owner.completion_navigation(&json!({"owner":"left","operation":"navigate","id":id,"target":{"kind":"shell","command":"bad"}})).is_err());
    owner.project_snapshot(&mut sample(json!([]),"b"));
    assert!(owner.completion_navigation(&params).is_err());
    owner.project_snapshot(&mut sample(json!([]),"a"));
    assert!(owner.completion_navigation(&params).is_ok());
    owner.completion_inbox(json!({"owner":"left","operation":"release"})).unwrap();
    assert!(owner.completion_navigation(&params).is_err());
}
