//! Durable unread completion identifiers. Core alone proves completion. The
//! host observes centrally even with no HUD page; windows only lease and ack.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

const KEEP_MS: i64 = 7 * 86_400_000;
const MAX_UNREAD: usize = 256;
const MAX_SEEN: usize = 2048;
const LEASE_MS: i64 = 6_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    id: String,
    source: String,
    session_id: String,
    turn_id: String,
    at: String,
    time: i64,
    scope: String,
    // Only protocol-owned identifier targets. Never persist path, URL or command.
    navigation: Option<Value>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct SavedInbox {
    version: u8,
    unread: Vec<Record>,
    seen: BTreeMap<String, i64>,
    dropped: usize,
}
pub(super) struct Inbox {
    path: PathBuf,
    saved: SavedInbox,
    snapshot: Value,
    enabled: bool,
    owner: Option<(String, i64)>,
    error: Option<String>,
    read_failed: bool,
}
fn hash(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
}
fn stamp(value: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value.as_str()?).ok().map(|t| t.timestamp_millis())
}
fn scope(snapshot: &Value, source: &str, session: &str) -> Option<String> {
    // A local transcript is not proven to belong to whichever quota account
    // finishes loading. Its stable session ID remains its identity throughout.
    if source != "grok-bot" {
        return Some(hash(&json!([snapshot["mode"], source, "local-session", session])));
    }
    // Grok's session IDs already encode hash(account slot). Match the exact
    // account identity derived by agents::account::legacy_unresolved, rather
    // than assigning a reminder to an arbitrary currently loaded quota row.
    let slot=session.strip_prefix("grok-bot:")?.split_once(':')?.0;
    if slot.len()!=64 || !slot.bytes().all(|b|b.is_ascii_hexdigit()){return None;}
    let expected=grok_account(slot);
    snapshot["accounts"].as_array().into_iter().flatten()
        .any(|a|a["provider"]==source&&a["id"]==expected&&a["current"]!=false)
        .then(||hash(&json!([snapshot["mode"],source,"bot-slot",slot])))
}
fn grok_account(slot_hash:&str)->String {
    let home=format!("home:grok-bot:{slot_hash}");
    let mut digest=Sha256::new();
    for part in ["Grok","",&home]{digest.update(format!("{}:",part.len()).as_bytes());digest.update(part.as_bytes());}
    format!("account:{:x}",digest.finalize())
}
fn safe_navigation(value: &Value) -> Option<Value> {
    let kind = text(&value["kind"])?;
    if !["codex-thread", "grok-bot-agent", "antigravity-conversation", "claude-desktop", "deepseek-session"].contains(&kind) {
        return None;
    }
    let id = text(&value["id"])?;
    Some(json!({"kind":kind,"id":id}))
}
fn record(snapshot: &Value, event: &Value, now: i64) -> Option<Record> {
    if event["kind"] != "completion" || event["islandAlert"] == false { return None; }
    let source = text(&event["source"])?;
    let session = text(&event["sessionId"])?;
    let turn = text(&event["turnId"])?;
    let time = stamp(&event["at"])?;
    if time > now + 5_000 || time < now - KEEP_MS { return None; }
    let scope = scope(snapshot, source, session)?;
    let id = format!("completion:{}", hash(&json!([source,session,turn,event["at"]])));
    Some(Record { id, source:source.into(), session_id:session.into(), turn_id:turn.into(),
        at:event["at"].as_str()?.into(), time, scope, navigation:safe_navigation(&event["navigationTarget"]) })
}
impl Inbox {
    pub fn new(directory: PathBuf) -> Self {
        let path = directory.join("completion-inbox.json");
        let loaded = if path.exists() {
            fs::read(&path).map_err(|e|e.to_string()).and_then(|b| {
                if b.len() > 2_000_000 { return Err("Completion inbox exceeds its size limit".into()); }
                let saved: SavedInbox=serde_json::from_slice(&b).map_err(|_|"Completion inbox is unreadable".to_string())?;
                if saved.version != 1 {return Err("Completion inbox version is unsupported".into());}
                Ok(saved)
            })
        } else { Ok(SavedInbox {version:1,..Default::default()}) };
        let read_failed=loaded.is_err();
        let error=loaded.as_ref().err().cloned();
        Self {path,saved:loaded.unwrap_or_else(|_|SavedInbox{version:1,..Default::default()}),
            snapshot:Value::Null,enabled:true,owner:None,error,read_failed}
    }
    fn persist(&mut self) -> Result<(),String> {
        if self.read_failed { return Err(self.error.clone().unwrap_or("Completion inbox is unreadable".into())); }
        if let Some(parent)=self.path.parent(){fs::create_dir_all(parent).map_err(|e|e.to_string())?;}
        let temporary=self.path.with_extension("json.tmp");
        let bytes=serde_json::to_vec(&self.saved).map_err(|e|e.to_string())?;
        fs::write(&temporary,bytes).and_then(|_|fs::rename(&temporary,&self.path)).map_err(|e|e.to_string())
    }
    fn trim(&mut self, now:i64) {
        self.saved.unread.retain(|r|r.time>=now-KEEP_MS && r.time<=now+5_000);
        self.saved.seen.retain(|_,at|*at>=now-KEEP_MS && *at<=now+5_000);
        if self.saved.unread.len()>MAX_UNREAD {
            let extra=self.saved.unread.len()-MAX_UNREAD;
            self.saved.dropped+=extra; self.saved.unread.drain(..extra);
        }
        let mut protected=self.saved.unread.iter().map(|r|r.id.clone()).collect::<HashSet<_>>();
        protected.extend(self.snapshot["islandEvents"]["items"].as_array().into_iter().flatten().rev().take(32)
            .filter_map(|event|record(&self.snapshot,event,now).map(|r|r.id)));
        while self.saved.seen.len()>MAX_SEEN {
            let Some(id)=self.saved.seen.iter().filter(|(id,_)|!protected.contains(*id)).min_by_key(|(_,at)|**at).map(|(id,_)|id.clone()) else {break;};
            self.saved.seen.remove(&id);
        }
        if self.saved.unread.is_empty(){self.saved.dropped=0;}
    }
    pub fn observe(&mut self,snapshot:&Value,enabled:bool,prefs:&Map<String,Value>,now:i64) {
        self.snapshot=snapshot.clone(); self.enabled=enabled;
        let before=self.saved.clone();
        self.trim(now);
        for event in snapshot["islandEvents"]["items"].as_array().into_iter().flatten() {
            let Some(row)=record(snapshot,event,now) else {continue};
            if self.saved.seen.contains_key(&row.id) || self.saved.unread.iter().any(|r|r.id==row.id) {continue;}
            self.saved.seen.insert(row.id.clone(),row.time);
            let allowed=enabled && snapshot["status"]["paused"]!=true
                && prefs.get(&row.source).is_none_or(|p|p["live"]!=false && p["reminders"]!=false);
            // Core already filters completions at/before its startup. A real
            // completion can arrive before its first projected snapshot.
            if allowed {self.saved.unread.push(row);}
        }
        if !enabled {self.saved.unread.clear();}
        self.trim(now);
        if serde_json::to_value(&before).ok()!=serde_json::to_value(&self.saved).ok() {
            if let Err(error)=self.persist(){
                // Keep unread events in memory, but do not pretend they are restart-safe.
                self.error=Some(format!("Completion reminders could not be saved: {error}"));
            } else {self.error=None;}
        }
    }
    fn event(&self,row:&Record)->Value {
        let live=self.snapshot["sessions"].as_array().into_iter().flatten().find(|s|s["source"]==row.source&&s["id"]==row.session_id);
        let mut event=self.snapshot["islandEvents"]["items"].as_array().into_iter().flatten()
            .find(|e|e["source"]==row.source&&e["sessionId"]==row.session_id&&e["turnId"]==row.turn_id&&e["at"]==row.at)
            .cloned().unwrap_or_else(||json!({"kind":"completion","source":row.source,"sessionId":row.session_id,"turnId":row.turn_id,"at":row.at}));
        event["id"]=json!(row.id);
        // The original turn's safe destination wins over the session's newer target.
        event["navigationTarget"]=safe_navigation(row.navigation.as_ref().unwrap_or(&Value::Null)).unwrap_or(Value::Null);
        event["reminderNavigation"]=json!(!event["navigationTarget"].is_null());
        if let Some(session)=live {
            for key in ["title","model","botIdentity"] {if let Some(value)=session.get(key){event[key]=value.clone();}}
            if event["navigationTarget"].is_null() && !session["navigationTarget"].is_null(){event["navigationTarget"]=session["navigationTarget"].clone();}
        }
        event
    }
    pub fn navigation(&self, params:&Value, now:i64)->Result<Value,String> {
        if params.as_object().is_none_or(|p|p.keys().any(|key|!["owner","operation","id"].contains(&key.as_str()))) {
            return Err("Completion navigation accepts identifiers only".into());
        }
        let owner=text(&params["owner"]).ok_or("Completion owner missing")?;
        if !self.enabled || !self.owner.as_ref().is_some_and(|(id,until)|id==owner&&*until>now) {
            return Err("This window no longer owns the completion reminder".into());
        }
        let id=text(&params["id"]).ok_or("Completion ID missing")?;
        let row=self.saved.unread.iter().find(|row|row.id==id && row.time>=now-KEEP_MS
            && scope(&self.snapshot,&row.source,&row.session_id).as_ref()==Some(&row.scope)).ok_or("Completion reminder is no longer available for this account")?;
        safe_navigation(row.navigation.as_ref().unwrap_or(&Value::Null)).ok_or("This completion has no saved native destination".into())
    }
    pub fn request(&mut self,params:Value,now:i64)->Result<Value,String> {
        if self.error.is_some() && !self.read_failed {
            if self.persist().is_ok() {self.error=None;}
        }
        let owner=text(&params["owner"]).ok_or("Completion window identity is missing")?.to_string();
        let operation=params["operation"].as_str().unwrap_or("sync");
        let holds=self.owner.as_ref().is_some_and(|(id,until)|id==&owner&&*until>now);
        if operation=="release" {
            if holds {self.owner=None;}
            return Ok(json!({"items":[],"error":self.error,"dropped":self.saved.dropped}));
        }
        if matches!(operation,"dismiss"|"dismiss_all") {
            if !holds {return Err("This window no longer owns the completion reminder".into());}
            let ids=params["ids"].as_array().ok_or("Completion IDs are required")?.iter().filter_map(Value::as_str).collect::<HashSet<_>>();
            if self.saved.unread.iter().any(|row|ids.contains(row.id.as_str())
                && scope(&self.snapshot,&row.source,&row.session_id).as_ref()!=Some(&row.scope)) {
                return Err("Completion reminder belongs to another or unavailable account".into());
            }
            let before=self.saved.clone();
            self.saved.unread.retain(|row|!ids.contains(row.id.as_str()));
            if self.saved.unread.is_empty(){self.saved.dropped=0;}
            if let Err(error)=self.persist(){self.saved=before;self.error=Some(error.clone());return Err(format!("Completion reminder was not dismissed: {error}"));}
            self.error=None;
        } else if operation!="sync" {return Err("Unknown completion reminder operation".into());}
        if self.owner.as_ref().is_some_and(|(_,until)|*until<=now){self.owner=None;}
        if operation=="sync" && params["eligible"]!=true && holds {self.owner=None;}
        if self.enabled && params["eligible"]==true && (self.owner.as_ref().is_some_and(|(id,_)|id==&owner) || self.owner.is_none()) {
            self.owner=Some((owner.clone(),now+LEASE_MS));
        }
        let owns=self.owner.as_ref().is_some_and(|(id,_)|id==&owner);
        let items=if owns&&self.enabled {
            self.saved.unread.iter().filter(|row|scope(&self.snapshot,&row.source,&row.session_id).as_ref()==Some(&row.scope)).map(|row|self.event(row)).collect::<Vec<_>>()
        } else {vec![]};
        Ok(json!({"items":items,"error":self.error,"dropped":self.saved.dropped}))
    }
}

#[cfg(test)]
mod tests;
