//! Settings behavior: shared layout inheritance is independent of screen style.
use super::Presentation;
use serde_json::json;

#[test]
fn main_agent_choice_is_global_and_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let owner = Presentation::new(directory.path().to_owned()).unwrap();
    owner.update(json!({"screen":"left","patch":{"mainAgent":"codex"}})).unwrap();
    assert_eq!(owner.view()["store"]["global"]["mainAgent"], "codex");
    assert!(owner.view()["store"]["screens"]["left"].get("mainAgent").is_none());
    let reopened = Presentation::new(directory.path().to_owned()).unwrap();
    assert_eq!(reopened.view()["store"]["global"]["mainAgent"], "codex");
    reopened.update(json!({"screen":"right","patch":{"mainAgent":""}})).unwrap();
    assert_eq!(Presentation::new(directory.path().to_owned()).unwrap().view()["store"]["global"]["mainAgent"], "");
}

#[test]
fn completion_drop_setting_is_global_and_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let owner = Presentation::new(directory.path().to_owned()).unwrap();
    owner.update(json!({"screen":"left","patch":{"completionDrops":false}})).unwrap();
    let view = owner.view();
    assert_eq!(view["store"]["global"]["completionDrops"], false);
    assert!(view["store"]["screens"]["left"].get("completionDrops").is_none());
    let reopened = Presentation::new(directory.path().to_owned()).unwrap();
    assert_eq!(reopened.view()["store"]["global"]["completionDrops"], false);
    reopened.update(json!({"screen":"right","patch":{"completionDrops":true}})).unwrap();
    let reopened_again = Presentation::new(directory.path().to_owned()).unwrap();
    assert_eq!(reopened_again.view()["store"]["global"]["completionDrops"], true);
    assert!(reopened_again.view()["store"]["screens"]["right"].get("completionDrops").is_none());
}

#[test]
fn shared_edits_keep_screen_style_independent_and_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let owner = Presentation::new(directory.path().to_owned()).unwrap();
    owner.update(json!({"screen":"left","layoutScope":"effective","patch":{
        "panelWidth":720,"quotaDetails":false,"glowBrightness":0.8
    }})).unwrap();
    let view = owner.view();
    assert_eq!(view["sharedLayout"]["style"]["width"], 720);
    assert!(view["sharedLayout"]["hiddenSections"].as_array().unwrap().contains(&json!("blocks")));
    assert!(view["layouts"]["left"].is_null());
    assert_eq!(view["store"]["screens"]["left"]["glowBrightness"], 0.8);
    assert!(view["store"]["screens"]["right"].is_null());
    let reopened = Presentation::new(directory.path().to_owned()).unwrap();
    assert_eq!(reopened.view()["sharedLayout"], view["sharedLayout"]);
    assert_eq!(reopened.view()["store"], view["store"]);
}

#[test]
fn detached_screen_can_rejoin_shared_layout_without_copying_it() {
    let directory = tempfile::tempdir().unwrap();
    let owner = Presentation::new(directory.path().to_owned()).unwrap();
    owner.set_layout(json!({"screen":"all","layout":{"style":{"width":720}}})).unwrap();
    owner.set_layout(json!({"screen":"left","shared":false})).unwrap();
    assert_eq!(owner.view()["layouts"]["left"]["style"]["width"], 720);
    owner.update(json!({"screen":"left","layoutScope":"effective","patch":{"panelWidth":900}})).unwrap();
    assert_eq!(owner.view()["layouts"]["left"]["style"]["width"], 900);
    assert_eq!(owner.view()["sharedLayout"]["style"]["width"], 720);
    owner.set_layout(json!({"screen":"left","shared":true})).unwrap();
    assert!(owner.view()["layouts"].get("left").is_none());
    let reopened = Presentation::new(directory.path().to_owned()).unwrap();
    reopened.update(json!({"screen":"right","layoutScope":"effective","patch":{"panelWidth":760}})).unwrap();
    assert_eq!(reopened.view()["sharedLayout"]["style"]["width"], 760);
    assert!(reopened.view()["layouts"].get("left").is_none());
    assert!(reopened.view()["layouts"].get("right").is_none());
}

#[test]
fn a_failed_shared_layout_save_restores_the_in_memory_choice() {
    let directory = tempfile::tempdir().unwrap();
    let owner = Presentation::new(directory.path().to_owned()).unwrap();
    owner.set_layout(json!({"screen":"left","layout":{"style":{"width":800}}})).unwrap();
    let before = owner.view();
    // A directory in place of the temporary file deterministically refuses the write.
    std::fs::create_dir(directory.path().join("presentation.json.tmp")).unwrap();
    assert!(owner.set_layout(json!({"screen":"left","layout":null})).is_err());
    assert_eq!(owner.view(), before);
    assert!(owner.update(json!({"screen":"right","layoutScope":"effective","patch":{"panelWidth":740}})).is_err());
    assert_eq!(owner.view(), before);
    let reopened = Presentation::new(directory.path().to_owned()).unwrap();
    assert_eq!(reopened.view()["layouts"], before["layouts"]);
    assert_eq!(reopened.view()["sharedLayout"], before["sharedLayout"]);
}
