// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Existing Antigravity local services publish their explicit run status. This
//! read-only adapter starts neither an application nor a model task.
use super::service::{self, Endpoint};
use agent_hud_agents::antigravity::activity;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn read_endpoints(endpoints: &[Endpoint]) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    for endpoint in endpoints {
        let body = endpoint.call("GetAllCascadeTrajectories", &json!({}))?;
        rows.extend(activity::parse(&body, crate::hooks::now_ms())?);
    }
    Ok(merge(rows))
}

// Multiple local services can expose the same conversation. A disagreement is
// unknown, never a reason to pick whichever service happened to finish last.
fn merge(rows: Vec<Value>) -> Vec<Value> {
    let mut unique = BTreeMap::<String, Value>::new();
    for row in rows {
        let Some(id) = row["id"].as_str().map(str::to_owned) else {
            continue;
        };
        if let Some(old) = unique.get_mut(&id) {
            if old["state"] != row["state"] {
                old["state"] = json!("unverified");
            } else if row["observedAt"].as_str() > old["observedAt"].as_str() {
                *old = row;
            }
        } else {
            unique.insert(id, row);
        }
    }
    unique.into_values().collect()
}

/// HTTP calls are bounded by the shared service's two-second timeout; discovery
/// has a twenty-second bound. Runtime must reject late generations and expire
/// the eight-second observations while discovery is stalled.
pub fn read() -> Result<Vec<Value>, String> {
    let known = service::endpoints(false)?;
    match read_endpoints(&known) {
        Ok(rows) => Ok(rows),
        Err(error) => {
            let current = service::endpoints(true)?;
            if current == known {
                return Err(error);
            }
            read_endpoints(&current)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicting_services_are_unverified_and_cannot_restore_running() {
        let rows = merge(vec![
            json!({"id":"antigravity:one","state":"running"}),
            json!({"id":"antigravity:two","state":"running"}),
            json!({"id":"antigravity:one","state":"idle"}),
            json!({"id":"antigravity:one","state":"running"}),
        ]);
        assert_eq!(rows[0]["state"], "unverified");
        assert_eq!(rows[1]["state"], "running");
    }

    #[test]
    fn matching_services_deduplicate_and_empty_service_withdraws_all_rows() {
        let row = json!({"id":"antigravity:one","state":"waiting"});
        assert_eq!(merge(vec![row.clone(), row.clone()]), vec![row]);
        assert!(merge(vec![]).is_empty());
    }

    #[test]
    fn matching_services_keep_the_newest_observation_without_extending_a_conflict() {
        let old = json!({"id":"antigravity:one","state":"running","observedAt":"2026-10-25T00:00:00Z","expiresAt":"2026-10-25T00:00:08Z"});
        let new = json!({"id":"antigravity:one","state":"running","observedAt":"2026-10-25T00:00:02Z","expiresAt":"2026-10-25T00:00:10Z"});
        assert_eq!(merge(vec![new.clone(), old.clone()]), vec![new.clone()]);
        assert_eq!(merge(vec![old.clone(), new.clone()]), vec![new.clone()]);
        let conflict =
            json!({"id":"antigravity:one","state":"idle","observedAt":"2026-10-25T00:00:01Z"});
        let result = merge(vec![old, conflict, new]);
        assert_eq!(result[0]["state"], "unverified");
        assert_eq!(result[0]["expiresAt"], "2026-10-25T00:00:08Z");
    }
}
