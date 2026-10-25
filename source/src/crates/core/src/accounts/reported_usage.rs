//! Official period totals are displayed separately from classified local calls.
//! They never enter the input/output ledger or API price calculations.
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_SAFE: u64 = 9_007_199_254_740_991;
fn time(value: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value.as_str()?).ok().map(|d| d.timestamp_millis())
}

pub(super) fn totals(records: &[Value]) -> HashMap<String, Value> {
    let mut groups = HashMap::<String, BTreeMap<i64, &Value>>::new();
    let mut invalid = HashSet::new();
    for row in records {
        if row["source"] != "grok-bot" || row.get("totalTokens").is_none() { continue; }
        let Some(id) = row["id"].as_str() else { continue; };
        let valid = row["totalTokens"].as_u64().is_some_and(|n| n <= MAX_SAFE)
            && time(&row["observedAt"]).is_some()
            && time(&row["periodStart"]).zip(time(&row["periodEnd"]))
                .zip(row["at"].as_i64()).is_some_and(|((start, end), at)| start < end && start <= at && at < end);
        if !valid { invalid.insert(id.to_owned()); continue; }
        let day = row["at"].as_i64().unwrap();
        if let Some(old) = groups.entry(id.into()).or_default().insert(day, row) {
            if old["totalTokens"] != row["totalTokens"] || old["periodStart"] != row["periodStart"]
                || old["periodEnd"] != row["periodEnd"] { invalid.insert(id.to_owned()); }
        }
    }
    groups.into_iter().filter_map(|(id, rows)| {
        if invalid.contains(&id) { return None; }
        let first = *rows.values().next()?;
        let mut total = 0_u64;
        for row in rows.values() {
            if row["periodStart"] != first["periodStart"] || row["periodEnd"] != first["periodEnd"] { return None; }
            total = total.checked_add(row["totalTokens"].as_u64()?)?;
            if total > MAX_SAFE { return None; }
        }
        Some((id, json!({"totalTokens":total,"periodStart":first["periodStart"],
            "periodEnd":first["periodEnd"],"observedAt":first["observedAt"],
            "source":"official-bot-usage","breakdownAvailable":false})))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: &str, day: i64, total: u64) -> Value {
        json!({"source":"grok-bot","id":id,"at":day,"totalTokens":total,
            "periodStart":"2026-10-20T00:00:00Z","periodEnd":"2026-10-24T00:00:00Z",
            "observedAt":"2026-10-24T01:00:00Z"})
    }
    #[test]
    fn bot_period_totals_are_not_input_tokens_and_duplicate_days_count_once() {
        let day = 1792454400000_i64;
        let rows = vec![row("a",day,12),row("a",day,12),row("a",day+86400000,8),row("b",day,0)];
        let report = totals(&rows);
        assert_eq!(report["a"]["totalTokens"],20);
        assert_eq!(report["b"]["totalTokens"],0);
        assert!(report["a"].get("input").is_none());
        assert_eq!(report["a"]["breakdownAvailable"],false);
    }
    #[test]
    fn conflicting_or_out_of_period_reports_are_not_partial_or_zero_readings() {
        let day = 1792454400000_i64;
        assert!(totals(&[row("a",day,12),row("a",day,13)]).is_empty());
        assert!(totals(&[row("a",0,12)]).is_empty());
        assert!(totals(&[row("a",day,MAX_SAFE),row("a",day+86400000,1)]).is_empty());
        let mut missing = row("a",day,12); missing["observedAt"]=Value::Null;
        assert!(totals(&[missing]).is_empty());
    }

    #[test]
    fn reported_usage_refreshes_bot_rows_without_changing_the_classified_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let mut core = crate::AgentHudCore::new(vec![], dir.path().join("hud"));
        core.set_account_projection(crate::AccountProjection {
            session_records: vec![json!({"source":"grok-bot","id":"bot-a","key":"local-call",
                "title":"Fixture Bot","at":chrono::Utc::now().timestamp_millis(),"input":7,"output":3})],
            ..Default::default()
        });
        let report = row("bot-a",1792454400000_i64,120);
        core.set_account_projection(crate::AccountProjection {session_records:vec![report.clone()],..Default::default()});
        let first = core.sessions(json!({"scope":"all"})).unwrap();
        let bot = first["items"].as_array().unwrap().iter().find(|r| r["id"]=="bot-a").unwrap();
        assert_eq!(bot["tokens"],10);
        assert_eq!(bot["reportedUsage"]["totalTokens"],120);
        let mut updated = report; updated["totalTokens"]=json!(150);
        core.set_account_projection(crate::AccountProjection {session_records:vec![updated],..Default::default()});
        let next = core.sessions(json!({"scope":"all"})).unwrap();
        assert!(next["revision"].as_u64()>first["revision"].as_u64());
        let bot = next["items"].as_array().unwrap().iter().find(|r| r["id"]=="bot-a").unwrap();
        assert_eq!(bot["tokens"],10);
        assert_eq!(bot["reportedUsage"]["totalTokens"],150);
        core.set_account_projection(crate::AccountProjection::default());
        let cleared = core.sessions(json!({"scope":"all"})).unwrap();
        let bot = cleared["items"].as_array().unwrap().iter().find(|r| r["id"]=="bot-a").unwrap();
        assert!(bot.get("reportedUsage").is_none());
        assert_eq!(bot["tokens"],10);
    }
}
