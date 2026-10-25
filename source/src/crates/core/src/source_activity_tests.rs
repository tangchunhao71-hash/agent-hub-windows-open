//! The real Antigravity SQLite reader, cached session heads, filtering/paging,
//! detail reconstruction and snapshots must agree on each ephemeral state.
use super::*;
use std::time::Duration;

const IDS: [&str; 3] = ["antigravity:fixture-a", "antigravity:fixture-b", "antigravity:fixture-c"];

fn varint(mut number: u64) -> Vec<u8> {
    let mut result = Vec::new();
    while number >= 128 { result.push((number as u8 & 127) | 128); number >>= 7; }
    result.push(number as u8);
    result
}
fn integer(field: u64, number: u64) -> Vec<u8> {
    let mut result = varint(field << 3); result.extend(varint(number)); result
}
fn bytes(field: u64, value: &[u8]) -> Vec<u8> {
    let mut result = varint((field << 3) | 2);
    result.extend(varint(value.len() as u64)); result.extend(value); result
}
fn fixture() -> (tempfile::TempDir, AgentHudCore) {
    let directory = tempfile::tempdir().unwrap();
    let sources = directory.path().join("antigravity/conversations");
    fs::create_dir_all(&sources).unwrap();
    let now = Utc::now().timestamp_millis();
    for (index, raw) in ["fixture-a", "fixture-b", "fixture-c"].iter().enumerate() {
        let db = Connection::open(sources.join(format!("{raw}.db"))).unwrap();
        db.execute_batch("CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY,data BLOB NOT NULL)").unwrap();
        let mut usage = integer(2, 7); usage.extend(integer(3, 3));
        usage.extend(bytes(11, format!("response-{raw}").as_bytes()));
        let at = now - (3-index as i64)*60_000;
        let timestamp = integer(1,(at/1000) as u64);
        let mut metadata = bytes(4,&usage);
        metadata.extend(bytes(9,&bytes(4,&timestamp)));
        metadata.extend(bytes(19,b"fixture-model"));
        let mut turn = bytes(1,&metadata);
        turn.extend(bytes(4,format!("execution-{raw}").as_bytes()));
        db.execute("INSERT INTO gen_metadata(idx,data) VALUES (1,?1)",[turn]).unwrap();
    }
    let mut core = AgentHudCore::new(vec![SourceRoot {path:sources,provider:"antigravity"}],directory.path().join("hud"));
    let mut snapshot = core.request("refresh",json!({})).unwrap();
    for _ in 0..10 {
        if core.reads.pending.is_empty() { break; }
        snapshot = core.request("snapshot",json!({})).unwrap();
    }
    assert!(core.reads.pending.is_empty(),"Antigravity SQLite fixtures must finish indexing");
    assert_eq!(snapshot["totals"]["total"],30);
    assert_eq!(snapshot["sessions"].as_array().unwrap().len(),3);
    (directory,core)
}
fn activity(id:&str, state:&str, observed:i64, expires:i64) -> Value {
    json!({"source":"antigravity","id":id,"state":state,
        "observedAt":DateTime::<Utc>::from_timestamp_millis(observed).unwrap().to_rfc3339(),
        "expiresAt":DateTime::<Utc>::from_timestamp_millis(expires).unwrap().to_rfc3339()})
}
fn publish(core:&mut AgentHudCore, first:&str) {
    let now=Utc::now().timestamp_millis();
    core.set_source_activity(Some(IDS.iter().enumerate().map(|(index,id)|
        activity(id,if index==0 {first} else {"idle"},now,now+8_000)).collect()));
}
fn assert_no_success_alert(snapshot:&Value) {
    assert!(snapshot["islandEvents"]["items"].as_array().unwrap().iter()
        .all(|event| event["kind"]!="completion" || event["source"]!="antigravity"));
}
fn list(core:&mut AgentHudCore,state:Option<&str>,offset:u64,limit:u64) -> Value {
    let mut params=json!({"scope":"all","source":"antigravity","sort":"active","offset":offset,"limit":limit});
    if let Some(state)=state { params["states"]=json!([state]); }
    core.request("sessions",params).unwrap()
}
fn assert_views(core:&mut AgentHudCore, expected:&str) -> u64 {
    let snapshot=core.request("snapshot",json!({})).unwrap();
    assert_eq!(snapshot["totals"]["total"],30);
    let session=snapshot["sessions"].as_array().unwrap().iter().find(|row|row["id"]==IDS[0]).unwrap();
    assert_eq!(session["state"],expected);
    assert_eq!(session["tokens"],10);
    assert_no_success_alert(&snapshot);
    let detail=core.request("session_detail",json!({"id":IDS[0]})).unwrap();
    assert_eq!(detail["session"]["state"],expected);
    assert_eq!(detail["usage"]["total"],10);
    if expected=="unverified" { assert!(detail["session"].get("liveFacts").is_none()); }
    else { assert!(detail["session"]["liveFacts"]["turnId"].is_null()); }
    let page=list(core,Some(expected),0,1);
    let expected_count=if expected=="idle" {3} else {1};
    assert_eq!(page["total"],expected_count);
    assert_eq!(page["items"][0]["state"],expected);
    let all=list(core,None,0,10);
    assert_eq!(all["total"],3);
    let mut paged=Vec::new();
    for offset in 0..3 {
        let single=list(core,None,offset,1);
        assert_eq!(single["items"].as_array().unwrap().len(),1);
        assert_eq!(single["nextOffset"],if offset<2 {json!(offset+1)} else {Value::Null});
        paged.push(single["items"][0]["id"].clone());
    }
    assert_eq!(paged,all["items"].as_array().unwrap().iter().map(|row|row["id"].clone()).collect::<Vec<_>>());
    all["revision"].as_u64().unwrap()
}

#[test]
fn source_activity_lifecycle_updates_cached_filtered_pages_and_reconstructed_details() {
    let (_directory,mut core)=fixture();
    publish(&mut core,"running");
    let running=assert_views(&mut core,"running");
    assert_eq!(list(&mut core,Some("running"),0,1)["items"][0]["id"],IDS[0]);
    publish(&mut core,"waiting");
    let waiting=assert_views(&mut core,"waiting");
    assert!(waiting>running);
    assert_eq!(list(&mut core,Some("running"),0,1)["total"],0);
    assert_eq!(list(&mut core,None,0,1)["items"][0]["id"],IDS[0],"waiting outranks newer idle history before paging");
    publish(&mut core,"idle");
    let idle=assert_views(&mut core,"idle");
    assert!(idle>waiting);
    assert_eq!(list(&mut core,Some("waiting"),0,1)["total"],0);
    assert_eq!(list(&mut core,None,0,1)["activeCount"],0);
    assert_eq!(list(&mut core,None,0,1)["items"][0]["id"],IDS[2],"idle resumes history ordering");
    publish(&mut core,"running");
    assert_views(&mut core,"running");
    core.set_source_activity(Some(vec![]));
    let snapshot=core.request("snapshot",json!({})).unwrap();
    assert_eq!(snapshot["totals"]["total"],30);
    assert!(snapshot["sessions"].as_array().unwrap().iter().all(|row|row["state"]=="unverified"));
    assert_eq!(list(&mut core,Some("running"),0,1)["total"],0);
    assert_eq!(list(&mut core,Some("unverified"),0,1)["total"],3);
    assert_eq!(core.request("session_detail",json!({"id":IDS[0]})).unwrap()["session"]["state"],"unverified");
    assert_no_success_alert(&snapshot);
}

#[test]
fn source_activity_clock_expiry_invalidates_already_cached_views_without_new_rows() {
    let (_directory,mut core)=fixture();
    let now=Utc::now().timestamp_millis();
    let expires=now+1_000;
    let mut rows=IDS.iter().map(|id|activity(id,"idle",now,now+8_000)).collect::<Vec<_>>();
    rows[0]=activity(IDS[0],"running",now,expires);
    core.set_source_activity(Some(rows));
    let before=assert_views(&mut core,"running");
    // Let the existing observation expire; no replace, file change, refresh or
    // injected clock tells the session cache that its state has changed.
    let remaining=(expires-Utc::now().timestamp_millis()+5).max(0) as u64;
    std::thread::sleep(Duration::from_millis(remaining));
    let after=assert_views(&mut core,"unverified");
    assert!(after>before,"expiration must invalidate the shared session revision");
    assert_eq!(list(&mut core,Some("running"),0,1)["total"],0);
}

#[test]
fn source_activity_disable_withdraws_immediately_and_reenable_rejects_late_work() {
    let (_directory,mut core)=fixture();
    publish(&mut core,"running");
    assert_views(&mut core,"running");
    let disabled=core.request("set_provider",json!({"id":"antigravity","enabled":false})).unwrap();
    assert!(disabled["sessions"].as_array().unwrap().is_empty());
    assert_eq!(list(&mut core,None,0,10)["total"],0);
    assert!(core.request("session_detail",json!({"id":IDS[0]})).is_err());
    assert_no_success_alert(&disabled);
    // Simulate a result from before disabling arriving late at the core API.
    publish(&mut core,"running");
    assert_eq!(list(&mut core,None,0,10)["total"],0);
    let enabled=core.request("set_provider",json!({"id":"antigravity","enabled":true})).unwrap();
    assert_eq!(enabled["totals"]["total"],30);
    assert!(enabled["sessions"].as_array().unwrap().iter().all(|row|row["state"]!="running"));
    assert_eq!(list(&mut core,Some("running"),0,1)["total"],0);
    publish(&mut core,"running");
    assert_views(&mut core,"running");
}

#[test]
fn source_activity_new_service_session_is_visible_before_its_first_usage_log() {
    let directory=tempfile::tempdir().unwrap();
    let mut core=AgentHudCore::new(vec![],directory.path().join("hud"));
    let id="antigravity:service-only-before-first-response";
    let now=Utc::now().timestamp_millis();
    assert!(core.logs.is_empty());
    core.set_source_activity(Some(vec![activity(id,"running",now,now+8_000)]));
    let snapshot=core.request("snapshot",json!({})).unwrap();
    assert_eq!(snapshot["totals"]["total"],0);
    let session=snapshot["sessions"].as_array().unwrap().iter()
        .find(|row|row["id"]==id)
        .expect("a native running status must display before any model usage exists");
    assert_eq!(session["state"],"running");
    assert_eq!(session["tokens"],0);
    assert_eq!(session["navigationTarget"]["kind"],"antigravity-conversation");
    assert_eq!(session["navigationTarget"]["id"],"service-only-before-first-response");
    assert_no_success_alert(&snapshot);

    let page=list(&mut core,Some("running"),0,1);
    assert_eq!(page["total"],1);
    assert_eq!(page["activeCount"],1);
    assert_eq!(page["items"][0]["id"],id);
    assert_eq!(page["items"][0]["tokens"],0);
    assert!(page["nextOffset"].is_null());
    assert!(list(&mut core,Some("running"),1,1)["items"].as_array().unwrap().is_empty());
    let detail=core.request("session_detail",json!({"id":id})).unwrap();
    assert_eq!(detail["session"]["state"],"running");
    assert_eq!(detail["session"]["tokens"],0);
    assert_eq!(detail["usage"]["total"],0);
    assert!(detail["turns"].as_array().unwrap().is_empty());
    assert!(detail["calls"].as_array().unwrap().is_empty());
    assert!(core.logs.is_empty(),"displaying native status must not create a usage log");
    assert_eq!(core.database.as_ref().unwrap().query_row("SELECT COUNT(*) FROM ingestion_logs",[],|row|row.get::<_,i64>(0)).unwrap(),0);

    let waiting_now=Utc::now().timestamp_millis();
    core.set_source_activity(Some(vec![activity(id,"waiting",waiting_now,waiting_now+8_000)]));
    assert_eq!(list(&mut core,Some("running"),0,1)["total"],0);
    assert_eq!(list(&mut core,Some("waiting"),0,1)["items"][0]["id"],id);
    assert_eq!(core.request("session_detail",json!({"id":id})).unwrap()["session"]["state"],"waiting");
    core.set_source_activity(Some(vec![]));
    let withdrawn=core.request("snapshot",json!({})).unwrap();
    assert_eq!(withdrawn["totals"]["total"],0);
    assert!(withdrawn["sessions"].as_array().unwrap().is_empty());
    assert_eq!(list(&mut core,None,0,10)["total"],0);
    assert!(core.request("session_detail",json!({"id":id})).is_err());
    assert_no_success_alert(&withdrawn);
}
