//! Short-lived official Bot status. This projection never enters the ledger.
use super::*;

#[derive(Default)]
pub(super) struct States {
    observing: bool,
    rows: HashMap<String, Value>,
    observed_bots: HashSet<String>,
    settlements: HashMap<String, (Value, bool)>,
    pub revision: u64,
}
fn ms(value: &Value) -> Option<i64> { metering::date_ms(value.as_str()?) }
fn state(row: &Value, now: i64) -> &'static str {
    if ms(&row["expiresAt"]).is_none_or(|at| at <= now) { return "unverified"; }
    match row["state"].as_str() {
        Some("running") => "running", Some("waiting") => "waiting",
        Some("idle") => "idle", _ => "unverified",
    }
}
impl States {
    fn replace(&mut self, rows: Option<Vec<Value>>, now: i64) -> bool {
        let observing = rows.is_some();
        let mut next = HashMap::new();
        let mut duplicates = HashSet::new();
        if !observing { self.observed_bots.clear(); self.settlements.clear(); }
        for mut row in rows.into_iter().flatten().take(1000) {
            let Some(id) = row["id"].as_str().filter(|id| id.starts_with("grok-bot:") && id.len() <= 256) else { continue; };
            let Some(observed) = ms(&row["observedAt"]) else { continue; };
            let Some(expires) = ms(&row["expiresAt"]) else { continue; };
            if row["source"] != "grok-bot" || observed > now + 5_000 || expires <= observed
                || expires - observed > 15_000 || !matches!(row["state"].as_str(), Some("running"|"waiting"|"idle"|"unverified")) { continue; }
            let id = id.to_owned();
            // An unavailable/expired status is not a successful baseline read.
            let first = if state(&row, now) != "unverified" {
                self.observed_bots.insert(id.clone())
            } else { false };
            if state(&row, now) == "idle" && row["settlementId"].as_str().is_some_and(|s|s.starts_with("grok-settlement:") && s.len()<=128)
                && row["turnId"].as_str().is_some_and(|s|s.starts_with("grok-turn:") && s.len()<=128) && ms(&row["endedAt"]).is_some() {
                let key = format!("{id}:{}",row["settlementId"].as_str().unwrap());
                // A future server clock is normalized once per stable settlement,
                // never once per poll. The first successful Bot read is baseline.
                let (ended, publish) = self.settlements.entry(key).or_insert_with(|| (row["endedAt"].clone(),!first));
                row["endedAt"] = ended.clone(); row["completionAllowed"] = json!(*publish);
            }
            if next.insert(id.clone(), row).is_some() { duplicates.insert(id); }
        }
        for id in duplicates { next.remove(&id); }
        if self.observing == observing && self.rows == next { return false; }
        self.observing = observing;
        self.rows = next;
        self.revision = self.revision.wrapping_add(1);
        true
    }
    pub fn session_state(&self, source: &str, id: &str, now: i64) -> Option<&'static str> {
        (self.observing && source == "grok-bot").then(|| self.rows.get(id).map_or("unverified", |row| state(row, now)))
    }
    pub fn activity_at(&self, source: &str, id: &str, now: i64) -> Option<i64> {
        if source != "grok-bot" { return None; }
        let row = self.rows.get(id)?;
        match state(row, now) {
            "running"|"waiting" => ms(&row["observedAt"]),
            "idle" => ms(&row["endedAt"]).filter(|at| *at <= now),
            _ => None,
        }
    }
    pub fn next_change(&self, now: i64) -> Option<i64> {
        self.rows.values().filter_map(|row| ms(&row["expiresAt"])).filter(|at| *at > now).min()
    }
    pub fn fingerprint(&self, now: i64) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.revision.hash(&mut hash);
        let mut rows = self.rows.iter().map(|(id,row)| (id,state(row,now))).collect::<Vec<_>>();
        rows.sort_unstable(); rows.hash(&mut hash); hash.finish()
    }
    pub fn overlay(&self, session: &mut Value, now: i64) {
        let source = session["source"].as_str().unwrap_or("");
        let id = session["id"].as_str().unwrap_or("");
        let Some(current) = self.session_state(source,id,now) else { return; };
        let row = self.rows.get(id);
        let at = self.activity_at(source,id,now);
        if let Some(identity) = row.and_then(|r| identity(&r["botIdentity"])) { session["botIdentity"] = identity; }
        session["state"] = json!(current);
        let Some(row) = row.filter(|_| current != "unverified") else {
            session.as_object_mut().unwrap().remove("liveFacts"); return;
        };
        let activity = matches!(current,"running"|"waiting").then(|| {
            let work = row["activity"]["work"].as_str().filter(|w| matches!(*w,"shell"|"command"|"edit"|"read"|"agents"|"question"));
            json!({"work":work,"openCall":null,"toolAt":work.map(|_|row["observedAt"].clone()),
                "delegating":row["activity"]["delegating"]==true,"awaitsModel":work.is_none(),
                "retrying":row["activity"]["retrying"]==true,"compactingSince":null})
        });
        let outcome = match row["outcome"].as_str() {
            Some("success"|"completed") => Some("completed"), Some("failed") => Some("failed"),
            Some("cancelled"|"interrupted") => Some("interrupted"), _ => None,
        };
        let last_event = if matches!(current, "running"|"waiting") {
            row["observedAt"].clone()
        } else if ms(&row["endedAt"]).is_some() {
            row["endedAt"].clone()
        } else {
            session["updatedAt"].clone()
        };
        let turn = if row["completionAllowed"] == true {row["turnId"].clone()} else {Value::Null};
        session["liveFacts"] = json!({"turnId":turn,"turnStartedAt":null,"outcome":outcome,
            "endedAt":row["endedAt"],"limitedUntil":null,"observedAt":row["observedAt"],
            "lastEventAt":last_event,"activity":activity,"requests":[]});
        if let Some(at) = at.and_then(DateTime::<Utc>::from_timestamp_millis) { session["updatedAt"] = json!(at.to_rfc3339()); }
    }
}
fn identity(value: &Value) -> Option<Value> {
    let name = value["name"].as_str().filter(|s|!s.trim().is_empty() && s.chars().count()<=128 && !s.chars().any(char::is_control))?;
    let avatar = &value["avatar"];
    let style = |v:&Value| -> Option<Value> {
        let shape=v["shape"].as_str().filter(|s|s.len()<=32 && s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'))?;
        let color=v["color"].as_str().filter(|s|s.len()==7 && s.starts_with('#') && s[1..].bytes().all(|b|b.is_ascii_hexdigit()))?;
        Some(json!({"shape":shape,"color":color}))
    };
    let avatar = match avatar["kind"].as_str()? {
        "shape" => {let mut v=style(avatar)?;v["kind"]=json!("shape");v},
        "photo" => {
            let data=avatar["dataUrl"].as_str().filter(|s|s.len()<=2_800_000 &&
                ["data:image/png;base64,","data:image/jpeg;base64,","data:image/gif;base64,","data:image/webp;base64,"].iter().any(|prefix|s.starts_with(prefix)))?;
            json!({"kind":"photo","dataUrl":data,"fallback":style(&avatar["fallback"])?})
        }, _ => return None,
    };
    Some(json!({"name":name,"avatar":avatar}))
}
impl AgentHudCore {
    /// None stops observation; Some(empty) means the last read could not verify status.
    pub fn set_grok_activity(&mut self, rows: Option<Vec<Value>>) {
        if self.grok_activity.replace(rows, Utc::now().timestamp_millis()) {
            self.sessions_checked = None; self.sessions_fingerprint = None; self.listed_rows = None;
        }
    }
    pub fn source_collection_enabled(&self, source: &str) -> bool {
        self.enabled.get(source).copied().unwrap_or(true) && !self.demo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(now:i64,state:&str)->Value {json!({"source":"grok-bot","id":"grok-bot:fixture:bot-a","state":state,
        "observedAt":DateTime::<Utc>::from_timestamp_millis(now).unwrap().to_rfc3339(),
        "expiresAt":DateTime::<Utc>::from_timestamp_millis(now+15000).unwrap().to_rfc3339(),
        "activity":{"kind":"thinking","delegating":true}})}
    fn completed(now:i64,turn:&str)->Value {
        let mut value=row(now,"idle");
        value["turnId"]=json!(format!("grok-turn:{turn}"));value["settlementId"]=json!(format!("grok-settlement:{turn}"));
        value["endedAt"]=value["observedAt"].clone();value["outcome"]=json!("success");
        value["botIdentity"]=json!({"name":"Fixture Bot Full Name","avatar":{"kind":"shape","shape":"blob","color":"#00C972"}});
        value
    }
    #[test]
    fn confirmed_turns_remind_once_with_identity_and_never_replay_baseline() {
        let now=Utc::now().timestamp_millis();let mut states=States::default();let mut events=crate::island_events::Tracker::new();
        let mut session=json!({"source":"grok-bot","id":"grok-bot:fixture:bot-a","tokens":10});
        states.replace(Some(vec![completed(now,"old")]),now);states.overlay(&mut session,now);
        assert!(session["liveFacts"]["turnId"].is_null());
        assert_eq!(events.update(&[session.clone()],&[],&[],now)["items"].as_array().unwrap().len(),0);
        states.replace(Some(vec![row(now+1_000,"running")]),now+1_000);states.overlay(&mut session,now+1_000);
        assert_eq!(events.update(&[session.clone()],&[],&[],now+1_000)["items"].as_array().unwrap().len(),0);
        states.replace(Some(vec![completed(now+2_000,"new")]),now+2_000);states.overlay(&mut session,now+2_000);
        let first=events.update(&[session.clone()],&[],&[],now+2_000);
        assert_eq!(first["items"].as_array().unwrap().len(),1);
        assert_eq!(first["items"][0]["botIdentity"]["name"],"Fixture Bot Full Name");
        let ended=session["liveFacts"]["endedAt"].clone();
        states.replace(Some(vec![completed(now+3_000,"new")]),now+3_000);states.overlay(&mut session,now+3_000);
        assert_eq!(session["liveFacts"]["endedAt"],ended);
        assert_eq!(events.update(&[session.clone()],&[],&[],now+3_000)["items"].as_array().unwrap().len(),1);
        let mut failed=completed(now+4_000,"failure");failed["outcome"]=json!("failed");
        states.replace(Some(vec![failed]),now+4_000);states.overlay(&mut session,now+4_000);
        assert_eq!(events.update(&[session.clone()],&[],&[],now+4_000)["items"].as_array().unwrap().len(),1);
        states.replace(Some(vec![row(now+5_000,"idle")]),now+5_000);states.overlay(&mut session,now+5_000);
        assert_eq!(events.update(&[session],&[],&[],now+5_000)["items"].as_array().unwrap().len(),1);
    }
    #[test]
    fn identity_rejects_remote_or_executable_images_and_bad_names() {
        let shape=json!({"name":"Name","avatar":{"kind":"shape","shape":"blob","color":"#00C972"}});
        assert!(identity(&shape).is_some());
        let mut bad=shape.clone();bad["name"]=json!("bad\nname");assert!(identity(&bad).is_none());
        for url in ["https://example.test/avatar.png","data:image/svg+xml;base64,fixture"] {
            bad=shape.clone();bad["avatar"]=json!({"kind":"photo","dataUrl":url,"fallback":{"shape":"blob","color":"#00C972"}});
            assert!(identity(&bad).is_none());
        }
    }
    #[test]
    fn unavailable_or_expired_first_reads_cannot_publish_an_old_settlement() {
        let now=Utc::now().timestamp_millis();
        for initial in [row(now,"unverified"),completed(now-20_000,"old")] {
            let mut states=States::default();
            let mut session=json!({"source":"grok-bot","id":"grok-bot:fixture:bot-a"});
            states.replace(Some(vec![initial]),now);
            states.replace(Some(vec![completed(now+1_000,"old")]),now+1_000);
            states.overlay(&mut session,now+1_000);
            assert!(session["liveFacts"]["turnId"].is_null());
        }
    }
    #[test]
    fn explicit_status_expires_and_failure_clears_work_without_approvals() {
        let now=Utc::now().timestamp_millis(); let mut states=States::default();
        states.replace(Some(vec![row(now,"running")]),now);
        let mut session=json!({"source":"grok-bot","id":"grok-bot:fixture:bot-a","tokens":8});
        states.overlay(&mut session,now); assert_eq!(session["state"],"running");
        assert_eq!(session["liveFacts"]["activity"]["delegating"],true);
        assert_eq!(session["tokens"],8); assert_eq!(session["liveFacts"]["requests"],json!([]));
        let before=states.fingerprint(now); states.overlay(&mut session,now+15000);
        assert_eq!(session["state"],"unverified"); assert!(session.get("liveFacts").is_none());
        assert_ne!(before,states.fingerprint(now+15000));
        states.replace(Some(vec![row(now,"waiting")]),now); states.overlay(&mut session,now);
        assert_eq!(session["state"],"waiting"); assert_eq!(session["liveFacts"]["requests"],json!([]));
        states.replace(Some(vec![]),now); states.overlay(&mut session,now); assert_eq!(session["state"],"unverified");
        states.replace(None,now); assert!(states.session_state("grok-bot","x",now).is_none());
        session["updatedAt"] = json!("2020-01-01T00:00:00Z");
        states.replace(Some(vec![row(now,"idle")]),now); states.overlay(&mut session,now);
        assert_eq!(session["liveFacts"]["lastEventAt"],"2020-01-01T00:00:00Z");
    }
    #[test]
    fn lifecycle_changes_refresh_paged_sessions_and_keep_accounting_unchanged() {
        let dir=tempfile::tempdir().unwrap();let mut core=AgentHudCore::new(vec![],dir.path().join("hud"));
        let now=Utc::now().timestamp_millis();
        core.set_account_projection(AccountProjection{session_records:vec![json!({"source":"grok-bot","id":"grok-bot:fixture:bot-a","key":"roster:a","at":now-3*86400000,"input":7,"output":3})],..Default::default()});
        core.set_grok_activity(Some(vec![row(now,"running")]));
        let first=core.sessions(json!({"scope":"active","states":["running"]})).unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(),1);assert_eq!(first["activeCount"],1);
        assert_eq!(first["items"][0]["tokens"],10);
        core.set_grok_activity(Some(vec![row(now,"idle")]));
        let done=core.sessions(json!({"scope":"all"})).unwrap();
        assert_eq!(done["items"][0]["state"],"idle");assert_eq!(done["items"][0]["tokens"],10);
        assert!(done["revision"].as_u64()>first["revision"].as_u64());
        core.set_grok_activity(Some(vec![]));
        assert_eq!(core.sessions(json!({"scope":"all"})).unwrap()["items"][0]["state"],"unverified");
    }
    #[test]
    fn session_details_keep_the_same_bot_destination_as_the_reminder_snapshot() {
        let dir=tempfile::tempdir().unwrap();let mut core=AgentHudCore::new(vec![],dir.path().join("hud"));
        let now=Utc::now().timestamp_millis();let id="grok-bot:fixture:bot-a";
        core.set_account_projection(AccountProjection{session_records:vec![json!({"source":"grok-bot","id":id,"key":"roster:a","at":now,"input":7,"output":3})],..Default::default()});
        let list=core.sessions(json!({"scope":"all"})).unwrap();
        let target=json!({"kind":"grok-bot-agent","id":"bot-a"});
        assert_eq!(list["items"][0]["navigationTarget"],target);
        let detail=core.request("session_detail",json!({"id":id})).unwrap();
        assert_eq!(detail["session"]["navigationTarget"],target);
        assert_eq!(detail["session"]["id"],id);
    }
}
