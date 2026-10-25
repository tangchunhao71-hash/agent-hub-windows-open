//! Official Bot live state, kept ephemeral and independent of quota refreshes.
use super::{account::CACHE, cache};
use crate::account::{locate, Host};
use crate::l10n;
use serde_json::{json, Value};
use std::collections::HashSet;

// The official coordinator clamps a future server timestamp's age to zero.
// Permit at most its default 90s heartbeat window of clock skew; an implausible
// future timestamp invalidates only that Bot. The local lease remains 15s.
const MAX_FUTURE_SKEW_MS: i64 = 90_000;

fn invalid() -> String {
    l10n::text("Grok Bot 工作状态不可验证", "Grok Bot activity could not be verified")
}
fn millis(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}
fn iso(at: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(at).map(|d| d.to_rfc3339())
}
fn bool_field(value: &Value, key: &str) -> Result<bool, String> {
    match value.get(key) {
        None => Ok(false),
        Some(value) => value.as_bool().ok_or_else(invalid),
    }
}

// Only an authoritative settlement has a completion identity. A session ID,
// heartbeat time or a change from running to idle is not a completed turn.
fn settlement(live: &Value, slot: &str, bot: &str, observed: i64) -> Option<Value> {
    let raw = live["lastTurnSettlement"]["turnId"].as_str()
        .filter(|id| !id.is_empty() && id.len() <= 512 && id.trim() == *id
            && !id.chars().any(char::is_control))?;
    let outcome = match live["lastTurnSettlement"]["outcome"].as_str()? {
        "success" => "success", "failed"|"error" => "failed",
        "cancelled" => "cancelled", "interrupted" => "interrupted", _ => return None,
    };
    let at = millis(&live["lastTurnSettlement"]["settledAtMs"])
        .filter(|at| *at > 0 && *at <= observed.saturating_add(MAX_FUTURE_SKEW_MS))?;
    let raw_time = at.to_string();
    Some(json!({
        "turnId":format!("grok-turn:{}",cache::hash(&[slot,bot,raw])),
        "settlementId":format!("grok-settlement:{}",cache::hash(&[slot,bot,raw,&raw_time,outcome])),
        // This stable server value identifies the settlement across polls. The
        // normalized display time below must not be used as an event key.
        "serverSettledAtMs":at,"outcome":outcome,"endedAt":iso(at.min(observed))
    }))
}

pub fn read(host: &dyn Host) -> Result<Vec<Value>, String> {
    let directory = locate(host, &CACHE)?;
    let slot = cache::account_through(host, &directory).ok_or_else(invalid)?;
    let roster = cache::value_through(host, &directory, &cache::roster_key(&slot), 4)
        .ok_or_else(invalid)?;
    let rows = roster["rows"].as_array().ok_or_else(invalid)?;
    let known: HashSet<String> = rows.iter().filter_map(|row| {
        row["id"].as_str().filter(|id| cache::agent_id(id)).map(str::to_owned)
    }).collect();
    if known.len() != rows.len() || rows.len() > 500 { return Err(invalid()); }
    let body = host.native("grok-bot", "ReadBotActivity", &json!({}))?;
    if cache::account_through(host, &directory).as_deref() != Some(slot.as_str()) {
        return Err(invalid());
    }
    parse(&body, &slot, &known, chrono::Utc::now().timestamp_millis())
}

fn parse(body: &Value, slot: &str, known: &HashSet<String>, now: i64) -> Result<Vec<Value>, String> {
    if body["success"] != true || body["accountSlot"].as_str() != Some(slot) { return Err(invalid()); }
    let observed = body["observedAt"].as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis()).ok_or_else(invalid)?;
    if observed > now.saturating_add(5_000) || now.saturating_sub(observed) > 30_000 { return Err(invalid()); }
    let rows = body["statuses"].as_array().ok_or_else(invalid)?;
    let mut seen = HashSet::new();
    rows.iter().map(|row| {
        let raw = row["agentId"].as_str().filter(|id| known.contains(*id)).ok_or_else(invalid)?;
        if !seen.insert(raw) { return Err(invalid()); }
        let mut result = json!({"source":"grok-bot","id":cache::session_id(slot,raw),
            "state":"idle","observedAt":iso(observed),"expiresAt":iso(observed.saturating_add(15_000))});
        let reported_running = bool_field(row,"isRunning")?;
        let Some(live) = row.get("live").filter(|v| !v.is_null()) else {
            if reported_running { result["state"]=json!("unverified"); }
            return Ok(result);
        };
        if live["agentId"].as_str() != Some(raw) { return Err(invalid()); }
        let running = bool_field(live,"isRunning")? || bool_field(live,"hasRunningSubagents")?;
        let waiting = bool_field(live,"waiting")?;
        let updated = millis(&live["updatedAtMs"]).filter(|n| *n > 0).ok_or_else(invalid)?;
        if updated > observed.saturating_add(MAX_FUTURE_SKEW_MS) {
            result["state"]=json!("unverified");
            return Ok(result);
        }
        // The official coordinator defaults a missing/zero staleAfterMs to 90s.
        let stale_after = millis(&live["staleAfterMs"]).filter(|n| *n > 0).unwrap_or(90_000);
        let expires = updated.min(observed).saturating_add(stale_after).min(observed.saturating_add(15_000));
        if (running || waiting || reported_running) && expires <= now {
            result["state"]=json!("unverified");
            return Ok(result);
        }
        if running || waiting {
            result["state"]=json!(if waiting {"waiting"} else {"running"});
            result["expiresAt"]=json!(iso(expires));
            result["activity"]=json!({"retrying":bool_field(live,"isRetrying")?,
                "delegating":bool_field(live,"hasRunningSubagents")?});
            if let Some(kind @ ("thinking"|"tool")) = live["activity"]["kind"].as_str() {
                result["activity"]["kind"]=json!(kind);
            }
        } else if reported_running {
            // A positive aggregate without its own fresh live state is unknown.
            result["state"]=json!("unverified");
        } else if let Some(completed) = settlement(live,slot,raw,observed) {
            result.as_object_mut().unwrap().extend(completed.as_object().unwrap().clone());
        }
        Ok(result)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: i64=1_792_853_000_000;
    fn fixture() -> Value { json!({"success":true,"accountSlot":"fixture-slot","observedAt":iso(NOW),
        "statuses":[{"agentId":"bot-a","live":{"agentId":"bot-a","isRunning":true,"updatedAtMs":NOW-1_000,"staleAfterMs":"90000","activity":{"kind":"thinking"}}}]}) }
    fn parsed(body: &Value) -> Result<Vec<Value>, String> { parse(body,"fixture-slot",&HashSet::from(["bot-a".into()]),NOW) }
    #[test]
    fn explicit_running_waiting_and_completion_are_kept_out_of_token_usage() {
        let mut body=fixture(); let rows=parsed(&body).unwrap();
        assert_eq!(rows[0]["state"],"running");assert_eq!(rows[0]["activity"]["kind"],"thinking");
        assert_eq!(rows[0]["expiresAt"],json!(iso(NOW+15_000)));
        assert!(rows[0].get("tokens").is_none());assert!(rows[0].get("title").is_none());
        body["statuses"][0]["live"]["waiting"]=json!(true);
        assert_eq!(parsed(&body).unwrap()[0]["state"],"waiting");
        body["statuses"][0]["live"]["waiting"]=json!(false);
        body["statuses"][0]["live"]["isRunning"]=json!(false);
        body["statuses"][0]["live"]["lastTurnSettlement"]=json!({"turnId":"fixture-turn-1","outcome":"success","settledAtMs":NOW-20});
        let row=&parsed(&body).unwrap()[0];assert_eq!(row["state"],"idle");assert_eq!(row["outcome"],"success");
    }
    #[test]
    fn subagents_are_work_but_absent_stale_or_mismatched_live_state_is_not() {
        let mut body=fixture();body["statuses"][0]["live"]["isRunning"]=json!(false);
        body["statuses"][0]["live"]["hasRunningSubagents"]=json!(true);
        assert_eq!(parsed(&body).unwrap()[0]["state"],"running");
        body["statuses"][0]["live"]["updatedAtMs"]=json!(NOW-91_000);
        assert_eq!(parsed(&body).unwrap()[0]["state"],"unverified");
        body["statuses"][0]["live"]=Value::Null;body["statuses"][0]["isRunning"]=json!(true);
        assert_eq!(parsed(&body).unwrap()[0]["state"],"unverified");
        body["statuses"][0]["isRunning"]=json!(false);
        assert_eq!(parsed(&body).unwrap()[0]["state"],"idle");
    }
    #[test]
    fn account_bot_duplicate_wrong_types_and_old_responses_fail_closed() {
        let mut body=fixture();body["accountSlot"]=json!("wrong");assert!(parsed(&body).is_err());
        body=fixture();body["statuses"][0]["agentId"]=json!("unknown");assert!(parsed(&body).is_err());
        body=fixture();body["statuses"][0]["live"]["agentId"]=json!("other");assert!(parsed(&body).is_err());
        body=fixture();let duplicate=body["statuses"][0].clone();body["statuses"].as_array_mut().unwrap().push(duplicate);assert!(parsed(&body).is_err());
        body=fixture();body["statuses"][0]["live"]["isRunning"]=json!("true");assert!(parsed(&body).is_err());
        body=fixture();body["observedAt"]=json!(iso(NOW-31_000));assert!(parsed(&body).is_err());
    }
    #[test]
    fn future_server_clock_skew_uses_receipt_time_without_extending_the_lease() {
        let mut body=fixture();body["statuses"][0]["live"]["updatedAtMs"]=json!(NOW+21_000);
        let row=&parsed(&body).unwrap()[0];
        assert_eq!(row["state"],"running");
        assert_eq!(row["observedAt"],json!(iso(NOW)));
        assert_eq!(row["expiresAt"],json!(iso(NOW+15_000)));
    }
    #[test]
    fn a_far_future_timestamp_does_not_clear_other_bots() {
        let mut body=fixture();body["statuses"][0]["live"]["updatedAtMs"]=json!(NOW+MAX_FUTURE_SKEW_MS+1);
        body["statuses"].as_array_mut().unwrap().push(json!({"agentId":"bot-b","isRunning":false}));
        let rows=parse(&body,"fixture-slot",&HashSet::from(["bot-a".into(),"bot-b".into()]),NOW).unwrap();
        assert_eq!(rows[0]["state"],"unverified");assert_eq!(rows[1]["state"],"idle");
        assert_eq!(rows[0]["expiresAt"],json!(iso(NOW+15_000)));
    }
    #[test]
    fn a_finished_turn_with_supported_clock_skew_has_a_local_end_time() {
        let mut body=fixture();body["statuses"][0]["live"]["isRunning"]=json!(false);
        body["statuses"][0]["live"]["updatedAtMs"]=json!(NOW+21_000);
        body["statuses"][0]["live"]["lastTurnSettlement"]=json!({"turnId":"fixture-turn-1","outcome":"success","settledAtMs":NOW+20_000});
        let row=&parsed(&body).unwrap()[0];
        assert_eq!(row["state"],"idle");assert_eq!(row["outcome"],"success");
        assert_eq!(row["endedAt"],json!(iso(NOW)));
    }
    #[test]
    fn receiving_old_past_activity_again_never_renews_its_server_ttl() {
        let mut body=fixture();body["statuses"][0]["live"]["updatedAtMs"]=json!(NOW-90_001);
        assert_eq!(parsed(&body).unwrap()[0]["state"],"unverified");
        body["observedAt"]=json!(iso(NOW+10_000));
        let rows=parse(&body,"fixture-slot",&HashSet::from(["bot-a".into()]),NOW+10_000).unwrap();
        assert_eq!(rows[0]["state"],"unverified");
    }
    fn finished() -> Value {
        let mut body=fixture();body["statuses"][0]["live"]["isRunning"]=json!(false);
        body["statuses"][0]["live"]["lastTurnSettlement"]=json!({
            "turnId":"fixture-turn-1","outcome":"success","settledAtMs":NOW+20_000});
        body
    }
    #[test]
    fn settlement_identity_stays_stable_while_future_display_time_is_normalized() {
        let mut body=finished();let first=parsed(&body).unwrap().remove(0);
        body["observedAt"]=json!(iso(NOW+10_000));
        let next=parse(&body,"fixture-slot",&HashSet::from(["bot-a".into()]),NOW+10_000).unwrap().remove(0);
        assert_eq!(first["turnId"],next["turnId"]);
        assert_eq!(first["settlementId"],next["settlementId"]);
        assert_eq!(first["serverSettledAtMs"],NOW+20_000);
        assert_eq!(first["serverSettledAtMs"],next["serverSettledAtMs"]);
        assert_eq!(first["endedAt"],json!(iso(NOW)));
        assert_eq!(next["endedAt"],json!(iso(NOW+10_000)));
        assert!(first["turnId"].as_str().unwrap().starts_with("grok-turn:"));
        assert!(!serde_json::to_string(&first).unwrap().contains("fixture-turn-1"));
        assert!(first.get("startedAt").is_none());
    }
    #[test]
    fn identical_wire_turn_ids_are_scoped_to_the_account_and_bot() {
        let body=finished();let a=parsed(&body).unwrap().remove(0);
        let mut other_account=body.clone();other_account["accountSlot"]=json!("other-slot");
        let b=parse(&other_account,"other-slot",&HashSet::from(["bot-a".into()]),NOW).unwrap().remove(0);
        let mut other_bot=body;other_bot["statuses"][0]["agentId"]=json!("bot-b");
        other_bot["statuses"][0]["live"]["agentId"]=json!("bot-b");
        let c=parse(&other_bot,"fixture-slot",&HashSet::from(["bot-b".into()]),NOW).unwrap().remove(0);
        assert_ne!(a["turnId"],b["turnId"]);assert_ne!(a["turnId"],c["turnId"]);
        assert_ne!(a["settlementId"],b["settlementId"]);assert_ne!(a["settlementId"],c["settlementId"]);
    }
    #[test]
    fn non_success_settlements_are_explicit_and_malformed_completions_are_absent() {
        for (wire,normalized) in [("failed","failed"),("error","failed"),("cancelled","cancelled"),("interrupted","interrupted")] {
            let mut body=finished();body["statuses"][0]["live"]["lastTurnSettlement"]["outcome"]=json!(wire);
            let row=parsed(&body).unwrap().remove(0);assert_eq!(row["outcome"],normalized);
            assert!(row.get("settlementId").is_some());assert_ne!(row["outcome"],"success");
        }
        for bad in [Value::Null,json!(""),json!("contains\ncontrol"),json!({"text":"not-an-id"})] {
            let mut body=finished();body["statuses"][0]["live"]["lastTurnSettlement"]["turnId"]=bad;
            let row=parsed(&body).unwrap().remove(0);assert_eq!(row["state"],"idle");
            assert!(row.get("outcome").is_none());assert!(row.get("turnId").is_none());
        }
        let mut body=finished();body["statuses"][0]["live"]["lastTurnSettlement"]["outcome"]=json!("working");
        assert!(parsed(&body).unwrap()[0].get("settlementId").is_none());
    }
    #[test]
    fn a_previous_settlement_during_work_or_an_idle_transition_is_not_a_completion() {
        let mut body=finished();body["statuses"][0]["live"]["isRunning"]=json!(true);
        let active=parsed(&body).unwrap().remove(0);assert_eq!(active["state"],"running");
        assert!(active.get("outcome").is_none());assert!(active.get("turnId").is_none());
        body["statuses"][0]["live"]["isRunning"]=json!(false);
        body["statuses"][0]["live"].as_object_mut().unwrap().remove("lastTurnSettlement");
        let idle=parsed(&body).unwrap().remove(0);assert_eq!(idle["state"],"idle");
        assert!(idle.get("outcome").is_none());assert!(idle.get("settlementId").is_none());
    }
}
