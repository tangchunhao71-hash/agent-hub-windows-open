// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Ephemeral local-service facts. They never enter the usage ledger or imply a
//! successful completion; the source must separately prove a settled turn.
use super::*;
use std::hash::{Hash, Hasher};

#[derive(Default)]
pub(super) struct States {
    observing: bool,
    rows: BTreeMap<String, Value>,
    revision: u64,
}
fn ms(value: &Value) -> Option<i64> {
    value.as_str().and_then(metering::date_ms)
}
fn state(row: &Value, now: i64) -> &'static str {
    if ms(&row["expiresAt"]).is_none_or(|at| at <= now) {
        return "unverified";
    }
    match row["state"].as_str() {
        Some("running") => "running",
        Some("waiting") => "waiting",
        Some("idle") => "idle",
        _ => "unverified",
    }
}
impl States {
    fn replace(&mut self, rows: Option<Vec<Value>>, now: i64) -> bool {
        let observing = rows.is_some();
        let mut next = BTreeMap::new();
        let mut duplicates = HashSet::new();
        for row in rows.into_iter().flatten().take(1000) {
            let Some(id) = row["id"]
                .as_str()
                .filter(|id| id.starts_with("antigravity:") && id.len() > 12 && id.len() <= 256)
            else {
                continue;
            };
            let (Some(observed), Some(expires)) = (ms(&row["observedAt"]), ms(&row["expiresAt"]))
            else {
                continue;
            };
            if row["source"] != "antigravity"
                || observed > now + 5_000
                || expires <= observed
                || expires - observed > 15_000
                || !matches!(
                    row["state"].as_str(),
                    Some("running" | "waiting" | "idle" | "unverified")
                )
            {
                continue;
            }
            let id = id.to_owned();
            if next.insert(id.clone(), row).is_some() {
                duplicates.insert(id);
            }
        }
        for id in duplicates {
            next.remove(&id);
        }
        if self.observing == observing && self.rows == next {
            return false;
        }
        self.observing = observing;
        self.rows = next;
        self.revision = self.revision.wrapping_add(1);
        true
    }
    pub fn session_state(&self, source: &str, id: &str, now: i64) -> Option<&'static str> {
        (self.observing && source == "antigravity").then(|| {
            self.rows
                .get(id)
                .map_or("unverified", |row| state(row, now))
        })
    }
    pub fn activity_at(&self, source: &str, id: &str, now: i64) -> Option<i64> {
        matches!(
            self.session_state(source, id, now),
            Some("running" | "waiting")
        )
        .then(|| self.rows.get(id).and_then(|row| ms(&row["observedAt"])))
        .flatten()
    }
    pub fn next_change(&self, now: i64) -> Option<i64> {
        self.rows
            .values()
            .filter_map(|row| ms(&row["expiresAt"]))
            .filter(|at| *at > now)
            .min()
    }
    pub fn fingerprint(&self, now: i64) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.revision.hash(&mut hash);
        for (id, row) in &self.rows {
            (id, state(row, now)).hash(&mut hash);
        }
        hash.finish()
    }
    /// A first request can be running before its first usage record is written.
    /// Display that official fact without creating an accounting log or charge.
    pub fn extra_sessions(&self, now: i64, existing: &HashSet<(String, String)>) -> Vec<Value> {
        if !self.observing {
            return vec![];
        }
        self.rows
            .iter()
            .filter_map(|(id, row)| {
                if existing.contains(&("antigravity".into(), id.clone()))
                    || !matches!(state(row, now), "running" | "waiting")
                {
                    return None;
                }
                let raw = id.strip_prefix("antigravity:")?;
                let mut session = json!({"id":id,"source":"antigravity","title":"Antigravity",
                "model":"Unknown","cwd":null,"updatedAt":row["observedAt"],
                "state":"unverified","tokens":0,"cost":null,"contextPercent":null,
                "logPath":null,"parentId":null,"usageStatus":"unavailable",
                "evidence":"local-service",
                "navigationTarget":{"kind":"antigravity-conversation","id":raw,"cwd":null}});
                self.overlay(&mut session, now);
                Some(session)
            })
            .collect()
    }
    pub fn overlay(&self, session: &mut Value, now: i64) {
        let source = session["source"].as_str().unwrap_or("");
        let id = session["id"].as_str().unwrap_or("");
        let Some(current) = self.session_state(source, id, now) else {
            return;
        };
        let row = self.rows.get(id);
        let at = self.activity_at(source, id, now);
        session["state"] = json!(current);
        // No stable final turn has been proved by the status service. In
        // particular, idle/expiry/failure must never synthesize a success alert.
        if current == "unverified" {
            session.as_object_mut().unwrap().remove("liveFacts");
            return;
        }
        let observed = row
            .map(|row| row["observedAt"].clone())
            .unwrap_or(Value::Null);
        let working = matches!(current, "running" | "waiting");
        session["liveFacts"] = json!({"turnId":null,"turnStartedAt":null,
            "outcome":null,"endedAt":null,"limitedUntil":null,"observedAt":observed,
            "lastEventAt":if working {observed.clone()} else {session["updatedAt"].clone()},
            "activity":working.then(||json!({"work":null,"openCall":null,"toolAt":null,
                "delegating":false,"awaitsModel":current=="running","retrying":false,
                "compactingSince":null})),"requests":[]});
        if let Some(at) = at.and_then(DateTime::<Utc>::from_timestamp_millis) {
            session["updatedAt"] = json!(at.to_rfc3339());
        }
    }
}
impl AgentHudCore {
    /// None disables this service; Some(empty) withdraws all prior positive facts.
    pub fn set_source_activity(&mut self, rows: Option<Vec<Value>>) {
        let rows = self
            .source_collection_enabled("antigravity")
            .then_some(rows)
            .flatten();
        if self
            .source_activity
            .replace(rows, Utc::now().timestamp_millis())
        {
            self.sessions_checked = None;
            self.sessions_fingerprint = None;
            self.listed_rows = None;
        }
    }
}

#[cfg(test)]
#[path = "source_activity_tests.rs"]
mod integration_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn row(now: i64, value: &str) -> Value {
        json!({"id":"antigravity:fixture","source":"antigravity","state":value,
            "observedAt":DateTime::<Utc>::from_timestamp_millis(now).unwrap().to_rfc3339(),
            "expiresAt":DateTime::<Utc>::from_timestamp_millis(now+15_000).unwrap().to_rfc3339()})
    }
    #[test]
    fn states_expire_withdraw_and_do_not_claim_completion_or_spend() {
        let now = Utc::now().timestamp_millis();
        let mut states = States::default();
        let mut session = json!({"id":"antigravity:fixture","source":"antigravity",
            "updatedAt":"2020-01-01T00:00:00Z","tokens":123});
        states.replace(Some(vec![row(now, "running")]), now);
        let running = states.fingerprint(now);
        states.overlay(&mut session, now);
        assert_eq!(session["state"], "running");
        assert_eq!(session["tokens"], 123);
        assert!(session["liveFacts"]["turnId"].is_null());
        assert_eq!(states.next_change(now), Some(now + 15_000));
        assert_ne!(running, states.fingerprint(now + 15_000));
        states.overlay(&mut session, now + 15_000);
        assert_eq!(session["state"], "unverified");
        states.replace(Some(vec![row(now, "idle")]), now);
        states.overlay(&mut session, now);
        assert_eq!(session["state"], "idle");
        assert!(session["liveFacts"]["outcome"].is_null());
        states.replace(Some(vec![]), now);
        assert_eq!(
            states.session_state("antigravity", "antigravity:fixture", now),
            Some("unverified")
        );
        states.replace(None, now);
        assert_eq!(
            states.session_state("antigravity", "antigravity:fixture", now),
            None
        );
    }
    #[test]
    fn invalid_duplicate_and_cross_source_facts_cannot_create_work() {
        let now = Utc::now().timestamp_millis();
        let mut states = States::default();
        let mut wrong = row(now, "running");
        wrong["source"] = json!("codex");
        states.replace(Some(vec![wrong]), now);
        assert!(states.rows.is_empty());
        states.replace(Some(vec![row(now, "running"), row(now, "idle")]), now);
        assert!(states.rows.is_empty());
        states.replace(Some(vec![row(now + 10_000, "running")]), now);
        assert!(states.rows.is_empty());
        assert_eq!(states.session_state("codex", "fixture", now), None);
    }
}
