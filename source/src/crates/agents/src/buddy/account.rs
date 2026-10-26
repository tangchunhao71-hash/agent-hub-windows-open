// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! WorkBuddy's official account summary. Package aggregates are authoritative;
//! resource details add dates only, never another copy of the same credits.
use crate::account::{account_id, status, Host, Reading};
use crate::l10n;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MONTHLY_FREE: &str = "TCACA_code_008_cfWoLwvjU4";

fn invalid() -> String {
    l10n::text(
        "WorkBuddy 返回的额度数据缺失或不一致",
        "WorkBuddy quota data is missing or inconsistent",
    )
}
fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
}
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n: &f64| n.is_finite() && *n >= 0.0)
}
fn iso(value: &Value) -> Option<String> {
    text(value)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.to_rfc3339())
}
fn epoch(value: &Value) -> Option<String> {
    value
        .as_i64()
        .filter(|ms| *ms > 0)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|d| d.to_rfc3339())
}

pub fn read(host: &dyn Host) -> Result<Reading, String> {
    // The platform adapter owns authentication, transport and OS availability.
    parse(&host.native("workbuddy", "ReadAccount", &json!({}))?)
}

fn parse(body: &Value) -> Result<Reading, String> {
    if body["success"] != true {
        return Err(invalid());
    }
    let uid = text(&body["account"]["uid"]).ok_or_else(invalid)?;
    let enterprise = match &body["account"]["enterpriseId"] {
        Value::Null => "",
        value => text(value).ok_or_else(invalid)?,
    };
    let id = account_id("workbuddy", &[uid, enterprise]);
    let observed = iso(&body["observedAt"]).ok_or_else(invalid)?;
    let observed_ms = chrono::DateTime::parse_from_rfc3339(&observed)
        .unwrap()
        .timestamp_millis();
    let summary = &body["summary"];
    let packages = summary["Packages"].as_array().ok_or_else(invalid)?;
    let mut reading = Reading::empty("workbuddy");
    let plan = text(&summary["SubscriptionPackageName"])
        .map(str::to_owned)
        .or_else(|| (summary["IsPaidUser"] == false).then(|| l10n::text("免费", "Free")));
    reading.account = Some(
        json!({"id":id,"provider":"workbuddy","identity":"account","plan":plan,"checkedAt":observed}),
    );

    // Repeated resource records must never add credits or contradictory dates.
    let mut resources = BTreeMap::<String, &Value>::new();
    let rows = body["freePackages"].as_array().ok_or_else(invalid)?;
    let mut free_codes = BTreeSet::new();
    {
        for row in rows {
            free_codes.insert(text(&row["PackageCode"]).ok_or_else(invalid)?.to_owned());
            let Some(key) = text(&row["ResourceId"]) else {
                continue;
            };
            if resources
                .insert(key.to_owned(), row)
                .is_some_and(|old| old != row)
            {
                return Err(invalid());
            }
        }
    }

    let mut unique = BTreeMap::<String, &Value>::new();
    for package in packages {
        let code = text(&package["PackageCode"]).ok_or_else(invalid)?;
        // Summary can also include paid add-ons. Only the official free-resource
        // inventory establishes which package aggregates belong in this reader.
        if !free_codes.contains(code) {
            continue;
        }
        if unique
            .insert(code.to_owned(), package)
            .is_some_and(|old| old != package)
        {
            return Err(invalid());
        }
    }
    let mut total = 0.0;
    let mut remaining = 0.0;
    let mut unknown_time = false;
    for (code, package) in unique {
        if text(&package["CapacityUnit"]) != Some("credits") {
            return Err(invalid());
        }
        let capacity = number(&package["CycleTotalCapacity"]).ok_or_else(invalid)?;
        let left = number(&package["CycleRemainCapacity"]).ok_or_else(invalid)?;
        let used = number(&package["CycleUsedCapacity"]).ok_or_else(invalid)?;
        let frozen = if package["CycleFrozenCapacity"].is_null() {
            0.0
        } else {
            number(&package["CycleFrozenCapacity"]).ok_or_else(invalid)?
        };
        let tolerance = 1e-6_f64.max(capacity * 1e-10);
        if left > capacity + tolerance
            || used > capacity + tolerance
            || (left + used + frozen - capacity).abs() > tolerance
        {
            return Err(invalid());
        }
        total += capacity;
        remaining += left;
        if !total.is_finite() || !remaining.is_finite() {
            return Err(invalid());
        }
        let matching = resources
            .values()
            .copied()
            .filter(|row| row["PackageCode"] == code)
            .collect::<Vec<_>>();
        let name = if code == MONTHLY_FREE {
            l10n::text("每月免费积分", "Monthly free credits")
        } else {
            matching
                .iter()
                .find_map(|row| text(&row["PackageName"]))
                .map(str::to_owned)
                .unwrap_or_else(|| l10n::text("资源积分", "Resource credits"))
        };
        let mut periods = Vec::new();
        for row in matching {
            // Unzoned CycleEndTime is not UTC. Only explicit normalized ISO
            // fields from the local helper or zoned official values are usable.
            let end = iso(&row["CycleEndAt"]).or_else(|| iso(&row["CycleEndTime"]));
            let start = iso(&row["CycleStartAt"]).or_else(|| iso(&row["CycleStartTime"]));
            let expires = iso(&row["ExpiredTime"]).or_else(|| epoch(&row["DeductionEndTime"]));
            unknown_time |= end.is_none() && text(&row["CycleEndTime"]).is_some();
            periods.push(json!({"id":account_id("workbuddy-resource", &[&id, text(&row["ResourceId"]).unwrap()]),
                "cycleStartAt":start,"cycleEndAt":end,"expiresAt":expires,
                "cycleEndLocal":if end.is_none(){text(&row["CycleEndTime"])}else{None}}));
        }
        let next = |field: &str| -> Option<String> {
            periods
                .iter()
                .filter_map(|p| iso(&p[field]))
                .filter_map(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| (d.timestamp_millis(), s))
                })
                .filter(|(at, _)| *at > observed_ms)
                .min_by_key(|(at, _)| *at)
                .map(|(_, s)| s)
        };
        // Only the confirmed monthly plan has a renewal cycle. Activity expiry
        // never masquerades as a fresh grant/reset of credits.
        let reset = (code == MONTHLY_FREE).then(|| next("cycleEndAt")).flatten();
        let expires = next("expiresAt");
        let percent = (capacity > 0.0).then(|| (used / capacity * 100.0).clamp(0.0, 100.0));
        reading.quotas.push(json!({"id":format!("{id}:{code}"),"vendor":"workbuddy","accountId":id,
            "name":name,"shortLabel":if code == MONTHLY_FREE {l10n::text("月免费", "Monthly free")}else{l10n::text("资源", "Credits")},
            "allModels":true,"usedPercent":percent,"status":percent.map(status).unwrap_or("unknown"),
            "nativeUsage":{"used":used,"capacity":capacity,"remaining":left,"frozen":frozen,"unit":"credits"},
            "resetAt":reset,"expiresAt":expires,"resourcePeriods":periods,"observedAt":observed}));
    }
    let mut notice = if reading.quotas.is_empty() {
        l10n::text(
            "WorkBuddy 未提供额度读数",
            "WorkBuddy did not report quota readings",
        )
    } else {
        format!(
            "{} {remaining:.2} / {total:.2}",
            l10n::text("剩余积分", "Credits left:")
        )
    };
    if unknown_time {
        notice.push_str(&l10n::text(
            "；部分刷新时间未知（缺少时区）",
            "; some refresh times unknown (no timezone)",
        ));
    }
    reading.notice = Some(notice);
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    fn fixture() -> Value {
        let mut packages = Vec::new();
        for (code, total, left, used) in [
            ("activity", "3691", "1826.01000017", "1864.98999983"),
            (MONTHLY_FREE, "500", "0", "500"),
            ("reward", "1150", "308.49000171", "841.50999829"),
        ] {
            packages.push(json!({"PackageCode":code,"CapacityUnit":"credits","CycleTotalCapacity":total,"CycleRemainCapacity":left,"CycleUsedCapacity":used,"CycleFrozenCapacity":"0"}));
        }
        json!({"success":true,"account":{"uid":"private-fixture-user"},"observedAt":"2026-10-24T12:04:58Z",
            "summary":{"Packages":packages,"IsPaidUser":false,"CreditGroups":[{"CycleTotalCapacity":"5341"}],"TotalCredits":"5341"},
            "freePackages":[{"ResourceId":"private-resource","PackageCode":MONTHLY_FREE,"CycleEndTime":"2026-10-31 23:59:59","DeductionEndTime":2048596295000_i64,"CycleCapacitySizePrecise":"500"},{"ResourceId":"activity-resource","PackageCode":"activity"},{"ResourceId":"reward-resource","PackageCode":"reward"}]})
    }
    #[test]
    fn official_summary_is_counted_once_with_exhausted_monthly_free() {
        let value = fixture();
        let mut host = FakeHost::new("/fixture");
        host.natives
            .insert(("workbuddy".into(), "ReadAccount".into()), Ok(value));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        let sum = |key: &str| {
            reading
                .quotas
                .iter()
                .map(|q| q["nativeUsage"][key].as_f64().unwrap())
                .sum::<f64>()
        };
        assert_eq!(sum("capacity"), 5341.0);
        assert!((sum("remaining") - 2134.50000188).abs() < 1e-8);
        let month = reading
            .quotas
            .iter()
            .find(|q| q["id"].as_str().unwrap().ends_with(MONTHLY_FREE))
            .unwrap();
        assert_eq!(month["usedPercent"], 100.0);
        assert_eq!(month["nativeUsage"]["remaining"], 0.0);
        assert_eq!(month["status"], "critical");
        assert!(month["resetAt"].is_null());
        assert!(month["expiresAt"].as_str().unwrap().starts_with("2034"));
        assert_eq!(
            month["resourcePeriods"][0]["cycleEndLocal"],
            "2026-10-31 23:59:59"
        );
        assert!(reading.billing.is_empty());
        assert!(!serde_json::to_string(&reading.account)
            .unwrap()
            .contains("private-fixture-user"));
        assert!(!serde_json::to_string(&reading.quotas)
            .unwrap()
            .contains("private-resource"));
    }
    #[test]
    fn repeats_do_not_double_count_and_conflicting_repeats_fail() {
        let mut body = fixture();
        let duplicate = body["summary"]["Packages"][0].clone();
        body["summary"]["Packages"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        let resource = body["freePackages"][0].clone();
        body["freePackages"].as_array_mut().unwrap().push(resource);
        assert_eq!(parse(&body).unwrap().quotas.len(), 3);
        body["summary"]["Packages"][3]["CycleRemainCapacity"] = json!("999");
        assert!(parse(&body).is_err());
        let mut body = fixture();
        let mut resource = body["freePackages"][0].clone();
        resource["DeductionEndTime"] = json!(1);
        body["freePackages"].as_array_mut().unwrap().push(resource);
        assert!(parse(&body).is_err());
    }
    #[test]
    fn paid_packages_and_empty_free_inventory_are_not_free_credits() {
        let mut body = fixture();
        body["summary"]["Packages"].as_array_mut().unwrap().push(json!({"PackageCode":"paid-only","CapacityUnit":"credits","CycleTotalCapacity":"9999","CycleRemainCapacity":"9999","CycleUsedCapacity":"0"}));
        assert_eq!(parse(&body).unwrap().quotas.len(), 3);
        body["freePackages"] = json!([]);
        assert!(parse(&body).unwrap().quotas.is_empty());
        body["freePackages"] = Value::Null;
        assert!(parse(&body).is_err());
    }
    #[test]
    fn invalid_or_absent_counters_never_become_zero_usage() {
        for bad in [
            Value::Null,
            json!(""),
            json!("NaN"),
            json!("inf"),
            json!("-1"),
            json!("9999"),
        ] {
            let mut body = fixture();
            body["summary"]["Packages"][0]["CycleRemainCapacity"] = bad;
            assert!(parse(&body).is_err());
        }
        let mut body = fixture();
        body["summary"]["Packages"] = json!([]);
        assert!(parse(&body).unwrap().quotas.is_empty());
        body["summary"]["Packages"] = Value::Null;
        assert!(parse(&body).is_err());
    }
    #[test]
    fn zero_capacity_is_unknown_and_frozen_credits_are_not_consumed() {
        let mut body = fixture();
        let package = &mut body["summary"]["Packages"][0];
        package["CycleTotalCapacity"] = json!("0");
        package["CycleRemainCapacity"] = json!("0");
        package["CycleUsedCapacity"] = json!("0");
        let reading = parse(&body).unwrap();
        let row = reading
            .quotas
            .iter()
            .find(|q| q["id"].as_str().unwrap().ends_with(":activity"))
            .unwrap();
        assert!(row["usedPercent"].is_null());
        assert_eq!(row["status"], "unknown");
        let package = &mut body["summary"]["Packages"][0];
        package["CycleTotalCapacity"] = json!("100");
        package["CycleRemainCapacity"] = json!("60");
        package["CycleUsedCapacity"] = json!("30");
        package["CycleFrozenCapacity"] = json!("10");
        let reading = parse(&body).unwrap();
        let row = reading
            .quotas
            .iter()
            .find(|q| q["id"].as_str().unwrap().ends_with(":activity"))
            .unwrap();
        assert_eq!(row["usedPercent"], 30.0);
        assert_eq!(row["nativeUsage"]["remaining"], 60.0);
    }
    #[test]
    fn identity_and_observation_are_required_and_enterprises_are_distinct() {
        let body = fixture();
        let original = parse(&body).unwrap().account.unwrap()["id"].clone();
        assert_eq!(parse(&body).unwrap().account.unwrap()["id"], original);
        let mut enterprise = body.clone();
        enterprise["account"]["enterpriseId"] = json!("enterprise-2");
        assert_ne!(parse(&enterprise).unwrap().account.unwrap()["id"], original);
        for path in ["uid", "observedAt"] {
            let mut body = fixture();
            if path == "uid" {
                body["account"][path] = Value::Null;
            } else {
                body[path] = json!("2026-10-24 12:00:00");
            }
            assert!(parse(&body).is_err());
        }
    }
    #[test]
    fn explicit_cycle_timezone_is_preserved_and_expiry_is_not_reset() {
        let mut body = fixture();
        body["freePackages"][0]["CycleEndAt"] = json!("2026-10-31T23:59:59+08:00");
        let reading = parse(&body).unwrap();
        let month = reading
            .quotas
            .iter()
            .find(|q| q["id"].as_str().unwrap().ends_with(MONTHLY_FREE))
            .unwrap();
        assert_eq!(month["resetAt"], "2026-10-31T23:59:59+08:00");
        assert_ne!(month["resetAt"], month["expiresAt"]);
        body["freePackages"][0]["PackageCode"] = json!("activity");
        let reading = parse(&body).unwrap();
        assert!(reading.quotas.iter().all(|q| q["resetAt"].is_null()));
    }
}
