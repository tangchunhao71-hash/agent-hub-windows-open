//! Official per-Bot daily totals, kept separate from classified token ledgers.
use super::cache;
use crate::account::Host;
use crate::l10n;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn invalid() -> String {
    l10n::text("Grok Bot 的分 Bot 用量格式无法识别", "Grok Bot per-Bot usage has an unsupported format")
}
fn date(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        let s=value.as_str()?;
        s.parse::<i64>().ok()
            .or_else(|| chrono::DateTime::parse_from_rfc3339(s).ok().map(|d|d.timestamp_millis()))
            .or_else(|| chrono::NaiveDate::parse_from_str(s,"%Y-%m-%d").ok()?.and_hms_opt(0,0,0).map(|d|d.and_utc().timestamp_millis()))
    })
}
fn iso(value: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value.as_str()?).ok().map(|d|d.timestamp_millis())
}
fn tokens(value: &Value) -> Option<u64> {
    value.as_u64().or_else(||value.as_str()?.parse().ok()).filter(|n|*n<=9_007_199_254_740_991)
}
pub(super) fn read(host: &dyn Host, slot: &str) -> Result<Vec<Value>, String> {
    parse(&host.native("grok-bot", "ReadBotUsage", &json!({}))?,slot)
}
fn parse(body: &Value,slot: &str) -> Result<Vec<Value>,String> {
    if body["success"]!=true||body["accountSlot"].as_str()!=Some(slot){return Err(invalid());}
    let start=iso(&body["periodStart"]).ok_or_else(invalid)?;
    let end=iso(&body["periodEnd"]).ok_or_else(invalid)?;
    let observed=iso(&body["observedAt"]).ok_or_else(invalid)?;
    if start>=end||end>observed||end-start>32*86_400_000 {return Err(invalid());}
    let mut days=BTreeMap::new();
    for row in body["dailySpend"].as_array().ok_or_else(invalid)? {
        let category=row["category"].as_str().filter(|s|cache::agent_id(s)).ok_or_else(invalid)?;
        let day=date(&row["day"]).ok_or_else(invalid)?;
        if day<start||day>end {return Err(invalid());}
        let total=tokens(&row["totalTokens"]).ok_or_else(invalid)?;
        if days.insert((category.to_owned(),day),total).is_some_and(|old|old!=total){return Err(invalid());}
    }
    Ok(days.into_iter().map(|((category,day),total)| {
        let unattributed=matches!(category.as_str(),"Other"|"unattributed");
        let id=if unattributed {format!("grok-bot-usage:{}:unattributed",cache::hash(&[slot]))}else{cache::session_id(slot,&category)};
        json!({"source":"grok-bot","id":id,"key":format!("grok-spend:{}:{day}",cache::hash(&[slot,&category])),
            "at":day,"totalTokens":total,"periodStart":body["periodStart"],"periodEnd":body["periodEnd"],"observedAt":body["observedAt"],
            "title":if unattributed{Some(l10n::text("未归属 Bot 用量","Unattributed Bot usage"))}else{None},
            "usageSource":"grok-bot-official-daily"})
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture()->Value {json!({"success":true,"accountSlot":"fixture-account","periodStart":"2026-10-17T00:00:00Z","periodEnd":"2026-10-24T12:00:00Z","observedAt":"2026-10-24T12:00:01Z","dailySpend":[{"category":"bot-1","day":"2026-10-23","totalTokens":"12345"},{"category":"bot-2","day":"2026-10-23","totalTokens":0}]})}
    #[test]
    fn bot_totals_have_stable_roster_ids_and_no_invented_breakdown(){
        let rows=parse(&fixture(),"fixture-account").unwrap();assert_eq!(rows.len(),2);
        assert_eq!(rows[0]["id"],cache::session_id("fixture-account","bot-1"));
        assert_eq!(rows[0]["totalTokens"],12345);assert_eq!(rows[1]["totalTokens"],0);
        for row in rows {for key in ["input","output","read","write"]{assert!(row.get(key).is_none());}}
    }
    #[test]
    fn wrong_account_missing_and_invalid_totals_are_rejected(){
        assert!(parse(&fixture(),"another-account").is_err());
        for bad in [Value::Null,json!(-1),json!(1.5),json!("NaN"),json!("9007199254740992")] {
            let mut body=fixture();body["dailySpend"][0]["totalTokens"]=bad;assert!(parse(&body,"fixture-account").is_err());
        }
    }
    #[test]
    fn daily_duplicates_do_not_double_count_and_conflicts_fail(){
        let mut body=fixture();let duplicate=body["dailySpend"][0].clone();body["dailySpend"].as_array_mut().unwrap().push(duplicate);
        assert_eq!(parse(&body,"fixture-account").unwrap().len(),2);
        body["dailySpend"][2]["totalTokens"]=json!(99);assert!(parse(&body,"fixture-account").is_err());
    }
    #[test]
    fn reported_period_and_unattributed_usage_remain_explicit(){
        let mut body=fixture();body["dailySpend"][0]["category"]=json!("Other");
        let row=parse(&body,"fixture-account").unwrap().remove(0);
        assert!(row["id"].as_str().unwrap().starts_with("grok-bot-usage:"));assert!(row["title"].is_string());
        body["periodEnd"]=json!("2026-11-24T12:00:00Z");assert!(parse(&body,"fixture-account").is_err());
    }
}
