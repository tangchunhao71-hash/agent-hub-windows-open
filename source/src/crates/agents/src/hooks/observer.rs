//! Session observers: a client's own extension or plugin writes lifecycle
//! records (Pi's extension, OpenCode's plugin). They carry metadata only.
use crate::{by_id, HookEvent};
use serde_json::{json, Value};

/// How a client's observer records a turn.
pub struct Observer {
    /// Where the plugin writes its turn records.
    pub turns: crate::Location,
    /// Where the plugin's script goes.
    pub script: crate::Location,
    /// The first line that marks the script as Agent HUD's own.
    pub marker: &'static str,
    /// The script, installed as it is written.
    pub body: &'static str,
    /// The key holding when the record was made.
    pub at_key: &'static str,
    /// Whether records carry the turn's state; otherwise every record is a
    /// completed turn.
    pub states: bool,
}

pub fn parse_observation(source: &str, record: &Value) -> Result<HookEvent, String> {
    let observer = by_id(source).and_then(|agent| agent.observer);
    let Some(observer) = observer.filter(|_| record.is_object() && record["version"] == 1) else {
        return Err("Unsupported session observer record".into());
    };
    let id = record["sessionID"]
        .as_str()
        .filter(|s| s.starts_with(&format!("{source}:")) && s.len() > source.len() + 1)
        .ok_or("The observer session identity is invalid")?;
    let turn = record["turnID"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("The observer turn identity is missing")?;
    let start = record["startedAtMs"]
        .as_i64()
        .filter(|s| *s > 0)
        .ok_or("The observer start timestamp is invalid")?;
    // OpenCode's earlier metadata-only observer emitted successful completion
    // records without a state or observation clock. Keep those readable after
    // upgrading its plugin; a malformed new state must not use this fallback.
    let legacy_completed = source == "opencode"
        && record.get("state").is_none()
        && record.get("observedAtMs").is_none()
        && record["completedAtMs"].is_i64();
    let at_key = if legacy_completed { "completedAtMs" } else { observer.at_key };
    let at = record[at_key]
        .as_i64()
        .filter(|s| *s >= start)
        .ok_or("The observer timestamp precedes its turn")?;
    let state = if !observer.states || legacy_completed {
        "completed"
    } else {
        match record["state"].as_str() {
            Some("running") => "running",
            Some("waitingForApproval") => "waiting",
            Some("completed") => "completed",
            Some("ended") => "ended",
            _ => return Err("The observer turn state is invalid".into()),
        }
    };
    let outcome = match (state, record["stopReason"].as_str()) {
        ("completed", _) => Some("completed"),
        ("ended", Some("aborted")) => Some("interrupted"),
        ("ended", Some("error")) => Some("failed"),
        _ => None,
    };
    // Only the extension's exact iTerm session target is supported here. An
    // arbitrary URL, command or a host claim in a record never becomes an action.
    let target = &record["navigationTarget"];
    let target = if target["kind"] == "iTermSession"
        && target["id"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 4096)
    {
        json!({"kind":"iterm-session","id":target["id"]})
    } else {
        Value::Null
    };
    Ok(HookEvent {
        source: source.into(),
        session_id: id.into(),
        at_ms: at,
        kind: "session-observation".into(),
        data: json!({"turnId":turn,"state":state,"startedAtMs":start,"observedAtMs":at,"title":record["title"].as_str(),"workspace":record["workspace"].as_str(),"sessionFile":record["sessionFile"].as_str(),"model":record["model"].as_str(),"providerId":record["providerID"].as_str(),"outcome":outcome,"navigationTarget":target}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        json!({"version":1,"sessionID":"opencode:fixture","turnID":"turn",
            "startedAtMs":1000,"completedAtMs":2000})
    }
    #[test]
    fn legacy_success_and_new_running_observations_share_the_contract() {
        let legacy = parse_observation("opencode", &fixture()).unwrap();
        assert_eq!(legacy.data["state"],"completed");
        assert_eq!(legacy.at_ms,2000);
        let mut running=fixture();running["state"]=json!("running");running["observedAtMs"]=json!(3000);
        let current=parse_observation("opencode",&running).unwrap();
        assert_eq!(current.data["state"],"running");
        assert!(current.data["outcome"].is_null());
        assert_eq!(current.at_ms,3000);
    }
    #[test]
    fn invalid_new_observations_never_fall_back_to_success() {
        let mut malformed=fixture();malformed["state"]=json!("bogus");
        assert!(parse_observation("opencode",&malformed).is_err());
        let mut missing=fixture();missing["observedAtMs"]=json!(3000);
        assert!(parse_observation("opencode",&missing).is_err());
        let mut reversed=fixture();reversed["completedAtMs"]=json!(500);
        assert!(parse_observation("opencode",&reversed).is_err());
    }
}
