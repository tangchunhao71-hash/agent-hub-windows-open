// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Read-only status from GetAllCascadeTrajectories. The installed service's
//! CascadeTrajectorySummary.status uses CascadeRunStatus (idle=1, running=2,
//! canceling=3, busy=4); its waiting steps use CortexStepStatus (waiting=9).
//! Summary prose, paths, credentials, and turn contents never enter these rows.
use serde_json::{json, Value};

pub const TTL_MS: i64 = 8_000;

fn status(value: &Value) -> Option<&'static str> {
    match (value.as_str(), value.as_u64()) {
        (Some("CASCADE_RUN_STATUS_IDLE"), _) | (_, Some(1)) => Some("idle"),
        (Some("CASCADE_RUN_STATUS_RUNNING" | "CASCADE_RUN_STATUS_BUSY"), _) | (_, Some(2 | 4)) => {
            Some("running")
        }
        // A cancellation in flight must stop advertising productive work. It
        // does not provide evidence of a successfully completed turn.
        (Some("CASCADE_RUN_STATUS_CANCELING"), _) | (_, Some(3)) => Some("idle"),
        _ => None,
    }
}

fn summary_state(summary: &Value) -> &'static str {
    if !summary.is_object() {
        return "unverified";
    }
    let Some(state) = status(&summary["status"]) else {
        return "unverified";
    };
    for key in [
        "killed",
        "interrupted",
        "hasActiveChildren",
        "hasRunningSubagents",
        "notFullyIdle",
    ] {
        if summary.get(key).is_some_and(|value| !value.is_boolean()) {
            return "unverified";
        }
    }
    if summary["killed"] == true || summary["interrupted"] == true {
        return "idle";
    }
    // Canceling does not become running again because the service still lists
    // children or waiting steps that are being torn down.
    if matches!(
        summary["status"].as_str(),
        Some("CASCADE_RUN_STATUS_CANCELING")
    ) || summary["status"] == 3
    {
        return "idle";
    }
    if let Some(waiting) = summary.get("waitingSteps") {
        let Some(waiting) = waiting.as_array() else {
            return "unverified";
        };
        if waiting.iter().any(|item| {
            item["step"]["status"] == "CORTEX_STEP_STATUS_WAITING" || item["step"]["status"] == 9
        }) {
            return "waiting";
        }
    }
    if state == "running"
        || summary["hasActiveChildren"] == true
        || summary["hasRunningSubagents"] == true
        || summary["notFullyIdle"] == true
    {
        "running"
    } else {
        "idle"
    }
}

/// Each successful call is a fresh observation of an existing local service,
/// not evidence inferred from a file's timestamp or a process being alive.
pub fn parse(body: &Value, now_ms: i64) -> Result<Vec<Value>, String> {
    if !body.is_object() || now_ms <= 0 {
        return Err("Antigravity activity summary is invalid".into());
    }
    let summaries = match body.get("trajectorySummaries") {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(value) => value
            .as_object()
            .ok_or("Antigravity activity summaries are invalid")?,
    };
    if summaries.len() > 16_384 {
        return Err("Antigravity activity summaries exceed the limit".into());
    }
    let expires = now_ms
        .checked_add(TTL_MS)
        .ok_or("Antigravity activity clock is invalid")?;
    Ok(summaries
        .iter()
        .filter_map(|(cascade, summary)| {
            if cascade.is_empty() || cascade.len() > 256 || cascade.chars().any(char::is_control) {
                return None;
            }
            Some(json!({
                "source":"antigravity", "id":format!("antigravity:{cascade}"),
                "state":summary_state(summary),
                "observedAt":crate::iso(now_ms), "expiresAt":crate::iso(expires)
            }))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(summary: Value) -> Value {
        parse(
            &json!({"trajectorySummaries":{"fixture":summary}}),
            1_800_000_000_000,
        )
        .unwrap()
        .remove(0)
    }

    #[test]
    fn official_string_and_numeric_states_are_equivalent() {
        for (name, number, expected) in [
            ("IDLE", 1, "idle"),
            ("RUNNING", 2, "running"),
            ("CANCELING", 3, "idle"),
            ("BUSY", 4, "running"),
        ] {
            assert_eq!(
                row(json!({"status":format!("CASCADE_RUN_STATUS_{name}")}))["state"],
                expected
            );
            assert_eq!(row(json!({"status":number}))["state"], expected);
        }
    }

    #[test]
    fn waiting_requires_an_explicit_waiting_step_and_does_not_grant_approval() {
        for waiting in [json!("CORTEX_STEP_STATUS_WAITING"), json!(9)] {
            let result = row(json!({"status":2,"waitingSteps":[{"step":{"status":waiting}}]}));
            assert_eq!(result["state"], "waiting");
            assert!(result.get("permission").is_none());
            assert!(result.get("completion").is_none());
        }
        assert_eq!(row(json!({"status":1,"waitingSteps":[]}))["state"], "idle");
        assert_eq!(
            row(json!({"status":2,"waitingSteps":[{"step":{"status":8}}]}))["state"],
            "running"
        );
    }

    #[test]
    fn explicit_active_children_keep_the_parent_running() {
        for key in ["hasActiveChildren", "hasRunningSubagents", "notFullyIdle"] {
            let mut summary = json!({"status":1});
            summary[key] = json!(true);
            assert_eq!(row(summary)["state"], "running");
        }
    }

    #[test]
    fn cancellation_and_interruption_never_announce_success_or_keep_working() {
        for summary in [
            json!({"status":3,"hasActiveChildren":true}),
            json!({"status":2,"killed":true}),
            json!({"status":2,"interrupted":true}),
        ] {
            let result = row(summary);
            assert_eq!(result["state"], "idle");
            assert_eq!(result.as_object().unwrap().len(), 5);
        }
    }

    #[test]
    fn unknown_or_malformed_states_fail_closed() {
        for summary in [
            json!({}),
            json!({"status":0}),
            json!({"status":99}),
            json!({"status":"RUNNING"}),
            json!({"status":2.5}),
            json!([]),
            json!({"status":2,"waitingSteps":{}}),
            json!({"status":2,"killed":"false"}),
        ] {
            assert_eq!(row(summary)["state"], "unverified");
        }
        assert!(parse(&json!([]), 1).is_err());
        assert!(parse(&json!({"trajectorySummaries":[]}), 1).is_err());
        assert!(parse(&json!({}), 0).is_err());
        assert!(parse(&json!({}), i64::MAX).unwrap().is_empty());
    }

    #[test]
    fn output_has_only_bound_identity_state_and_short_observation_lifetime() {
        let result =
            row(json!({"status":2,"summary":"private prose","workspaces":["private path"]}));
        assert_eq!(result["source"], "antigravity");
        assert_eq!(result["id"], "antigravity:fixture");
        let observed = crate::transcript::live::timestamp(&result["observedAt"]).unwrap();
        let expires = crate::transcript::live::timestamp(&result["expiresAt"]).unwrap();
        assert_eq!(expires - observed, TTL_MS);
        assert!(!result.to_string().contains("private"));
        assert_eq!(
            parse(
                &json!({"trajectorySummaries":{"":{},"bad\nkey":{},"valid":{"status":1}}}),
                1
            )
            .unwrap()
            .len(),
            1
        );
    }
}
