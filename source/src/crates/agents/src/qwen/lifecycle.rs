// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Qwen's explicit turn-attempt/result and request-lifecycle records.
//! API usage and the presence of an assistant message never establish completion.
use crate::transcript::at;
use crate::Log;
use serde_json::Value;

const MAX_FUTURE_MS: i64 = 90_000;

fn identity(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|v| !v.trim().is_empty() && v.len() <= 256)
}

fn clock(value: &Value, now: i64) -> Option<i64> {
    let latest = now.checked_add(MAX_FUTURE_MS).filter(|_| now > 0)?;
    // Do not let the generic numeric reader round fractions or saturate an
    // overflowing float into a plausible lifecycle timestamp.
    let timestamp = if value.is_number() {
        value.as_i64()
    } else if value.is_string() {
        at(value)
    } else {
        None
    }?;
    (timestamp > 0 && timestamp <= latest).then_some(timestamp)
}

fn epoch_clock(value: &Value, now: i64) -> Option<i64> {
    value.as_i64()?;
    clock(value, now)
}

fn start(log: &mut Log, id: &str, started: i64, observed: i64) {
    if started <= 0 || observed < started {
        return;
    }
    // An old phase can be appended after its terminal record with a newer
    // envelope clock. Its own start still belongs to the closed attempt, even
    // if another prompt is now last. A true retry must start after that end.
    if log
        .live
        .turns
        .iter()
        .rev()
        .find(|turn| turn.id.as_deref() == Some(id))
        .is_some_and(|last| !last.running && started <= last.observed)
    {
        return;
    }
    log.live
        .observe_source(id, true, Some(started), observed, None);
}

pub(super) fn observe(log: &mut Log, record: &Value) {
    observe_at(log, record, chrono::Utc::now().timestamp_millis());
}

fn observe_at(log: &mut Log, record: &Value, now: i64) {
    if record["type"] != "system" || !record["forkedFrom"].is_null() {
        return;
    }
    // If an envelope clock is present it must be valid too, even when the
    // phase below carries its own start/end clock. History has no lower age
    // bound: replaying valid old records must still reconstruct the turn.
    let observed = match record.get("timestamp") {
        Some(value) => match clock(value, now) {
            Some(timestamp) => Some(timestamp),
            None => return,
        },
        None => None,
    };
    let payload = &record["systemPayload"];
    match record["subtype"].as_str() {
        Some("turn_attempt") => {
            if let (Some(id), Some(timestamp)) = (
                identity(&record["promptId"]).or_else(|| identity(&record["daemonPromptId"])),
                observed,
            ) {
                start(log, id, timestamp, timestamp);
            }
        }
        Some("turn_result") => {
            let Some(id) = identity(&record["promptId"]).or_else(|| identity(&payload["promptId"]))
            else {
                return;
            };
            let Some(ended) = epoch_clock(&payload["endedAt"], now) else {
                return;
            };
            let started = match payload.get("startedAt") {
                Some(value) => match epoch_clock(value, now).filter(|start| *start <= ended) {
                    Some(start) => Some(start),
                    None => return,
                },
                None => None,
            };
            if let Some(value) = payload.get("cancelledAt") {
                if epoch_clock(value, now).is_none_or(|cancelled| {
                    cancelled > ended || started.is_some_and(|start| cancelled < start)
                }) {
                    return;
                }
            }
            let outcome = match payload["state"].as_str() {
                Some("completed") => "completed",
                Some("cancelled") => "interrupted",
                Some("error") => "failed",
                _ => return,
            };
            log.live.terminal(Some(id), ended, Some(outcome), None);
        }
        Some("ui_telemetry") => {
            let event = &payload["uiEvent"];
            if event["event.name"] != "request_lifecycle"
                || event["v"] != 1
                || event["kind"] != "request"
                || event["sessionId"].as_str() != log.id.strip_prefix("qwen:")
                || identity(&event["executionId"]).is_none()
                || !event["subagentId"].is_null()
            {
                return;
            }
            let (Some(id), Some(started)) = (
                identity(&event["promptId"]),
                epoch_clock(&event["startedAt"], now),
            ) else {
                return;
            };
            match event["phase"].as_str() {
                Some("started") => start(log, id, started, observed.unwrap_or(started)),
                Some("ended") => {
                    // A request may finish before tools or another request. Only the
                    // explicit turn_result closes the turn and can declare success.
                    if let Some(ended) =
                        epoch_clock(&event["endedAt"], now).filter(|end| *end >= started)
                    {
                        log.live.touch_source(id, ended);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_792_901_000_000;
    fn log() -> Log {
        Log::new("qwen/session.jsonl".into(), "qwen")
    }
    fn attempt(id: &str, time: i64) -> Value {
        json!({"sessionId":"s","type":"system","subtype":"turn_attempt","promptId":id,"timestamp":crate::iso(time)})
    }
    fn result(id: &str, state: &str, time: i64) -> Value {
        json!({"sessionId":"s","type":"system","subtype":"turn_result","systemPayload":{"promptId":id,"state":state,"endedAt":time}})
    }
    fn request(id: &str, phase: &str, time: i64) -> Value {
        json!({"sessionId":"s","type":"system","subtype":"ui_telemetry","systemPayload":{"uiEvent":{"event.name":"request_lifecycle","v":1,"kind":"request","sessionId":"s","executionId":"exec","promptId":id,"phase":phase,"startedAt":NOW,"endedAt":time,"outcome":"success"}}})
    }
    #[test]
    fn explicit_attempt_and_settlement_drive_work_without_usage() {
        for (state, outcome) in [
            ("completed", "completed"),
            ("cancelled", "interrupted"),
            ("error", "failed"),
        ] {
            let mut log = log();
            log.parse(&attempt("p", NOW));
            assert_eq!(log.live.session_state(NOW, "qwen", None), "running");
            log.parse(&result("p", state, NOW + 10));
            assert_eq!(log.live.session_state(NOW + 10, "qwen", None), "idle");
            assert_eq!(
                log.live.turns.back().unwrap().outcome.as_deref(),
                Some(outcome)
            );
            assert_eq!(log.usage.total(), 0);
        }
    }
    #[test]
    fn request_end_does_not_pretend_the_turn_or_tools_finished() {
        let mut log = log();
        log.parse(&request("p", "started", NOW));
        assert_eq!(log.live.session_state(NOW, "qwen", None), "running");
        log.parse(&request("p", "ended", NOW + 10));
        assert_eq!(log.live.session_state(NOW + 10, "qwen", None), "running");
        assert!(log.live.turns.back().unwrap().outcome.is_none());
        log.parse(&result("p", "completed", NOW + 20));
        assert_eq!(log.live.session_state(NOW + 20, "qwen", None), "idle");
    }
    #[test]
    fn duplicate_and_old_starts_cannot_resurrect_or_close_newer_work() {
        let mut log = log();
        log.parse(&attempt("old", NOW));
        log.parse(&result("old", "completed", NOW + 5));
        log.parse(&attempt("old", NOW));
        assert_eq!(log.live.session_state(NOW + 5, "qwen", None), "idle");
        log.parse(&attempt("new", NOW + 10));
        log.parse(&result("old", "completed", NOW + 15));
        assert_eq!(log.live.session_state(NOW + 15, "qwen", None), "running");
        assert_eq!(log.live.turns.back().unwrap().id.as_deref(), Some("new"));
    }
    #[test]
    fn copied_invalid_and_subagent_events_do_not_create_parent_work() {
        let mut log = log();
        let mut copied = attempt("p", NOW);
        copied["forkedFrom"] = json!("parent");
        log.parse(&copied);
        let mut other = request("p", "started", NOW);
        other["systemPayload"]["uiEvent"]["sessionId"] = json!("other");
        log.parse(&other);
        let mut child = request("p", "started", NOW);
        child["systemPayload"]["uiEvent"]["subagentId"] = json!("child");
        log.parse(&child);
        log.parse(&attempt("", NOW));
        log.parse(&result("p", "completed", -1));
        assert_eq!(log.live.session_state(NOW, "qwen", None), "idle");
        assert!(log.live.turns.is_empty());
    }
    #[test]
    fn work_expires_without_new_lifecycle_events() {
        let mut log = log();
        log.parse(&attempt("p", NOW));
        assert_eq!(
            log.live.session_state(NOW + 1_800_000, "qwen", None),
            "idle"
        );
    }

    #[test]
    fn far_future_clocks_cannot_create_renew_or_settle_work() {
        let future = NOW + MAX_FUTURE_MS + 1;
        let mut log = log();
        log.own_id("s");
        observe_at(&mut log, &attempt("p", future), NOW);
        let mut future_request = request("p", "started", future);
        future_request["systemPayload"]["uiEvent"]["startedAt"] = json!(future);
        observe_at(&mut log, &future_request, NOW);
        assert!(log.live.turns.is_empty());

        observe_at(&mut log, &attempt("p", NOW), NOW);
        let before = log.live.turns.back().unwrap().observed;
        observe_at(&mut log, &request("p", "ended", future), NOW);
        observe_at(&mut log, &result("p", "completed", future), NOW);
        let mut future_envelope = request("p", "started", NOW);
        future_envelope["timestamp"] = crate::iso(future);
        observe_at(&mut log, &future_envelope, NOW);
        assert_eq!(log.live.turns.back().unwrap().observed, before);
        assert!(log.live.turns.back().unwrap().running);
        assert!(log.live.turns.back().unwrap().outcome.is_none());
        assert_eq!(
            log.live.session_state(NOW + 1_800_000, "qwen", None),
            "idle"
        );

        let mut future_start = result("p", "completed", NOW + 10);
        future_start["systemPayload"]["startedAt"] = json!(future);
        observe_at(&mut log, &future_start, NOW);
        assert!(log.live.turns.back().unwrap().running);
    }

    #[test]
    fn malformed_and_overflowing_clocks_never_change_lifecycle() {
        for invalid in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!(i64::MAX),
            json!(u64::MAX),
            json!(null),
            json!("not-a-clock"),
        ] {
            let mut log = log();
            log.own_id("s");
            let mut bad_attempt = attempt("p", NOW);
            bad_attempt["timestamp"] = invalid.clone();
            observe_at(&mut log, &bad_attempt, NOW);
            assert!(log.live.turns.is_empty(), "attempt: {invalid}");
            observe_at(&mut log, &attempt("p", NOW), NOW);
            let mut bad_request = request("p", "ended", NOW + 10);
            bad_request["systemPayload"]["uiEvent"]["endedAt"] = invalid.clone();
            observe_at(&mut log, &bad_request, NOW);
            let mut bad_end = result("p", "completed", NOW + 10);
            bad_end["systemPayload"]["endedAt"] = invalid.clone();
            observe_at(&mut log, &bad_end, NOW);
            let mut bad_start = result("p", "completed", NOW + 10);
            bad_start["systemPayload"]["startedAt"] = invalid.clone();
            observe_at(&mut log, &bad_start, NOW);
            assert_eq!(
                log.live.turns.back().unwrap().observed,
                NOW,
                "renewal: {invalid}"
            );
            assert!(
                log.live.turns.back().unwrap().running,
                "terminal: {invalid}"
            );
        }
        let mut log = log();
        observe_at(&mut log, &attempt("p", NOW), i64::MAX);
        assert!(
            log.live.turns.is_empty(),
            "upper clock calculation must not overflow"
        );
    }

    #[test]
    fn bounded_clock_skew_and_historical_replay_remain_valid() {
        let mut log = log();
        let boundary = NOW + MAX_FUTURE_MS;
        observe_at(&mut log, &attempt("p", boundary), NOW);
        assert_eq!(log.live.turns.back().unwrap().observed, boundary);
        let mut terminal = result("p", "cancelled", boundary);
        terminal["systemPayload"]["startedAt"] = json!(boundary);
        terminal["systemPayload"]["cancelledAt"] = json!(boundary);
        observe_at(&mut log, &terminal, NOW);
        assert!(!log.live.turns.back().unwrap().running);

        let mut history = self::log();
        observe_at(&mut history, &attempt("old", NOW - 86_400_000), NOW);
        observe_at(
            &mut history,
            &result("old", "completed", NOW - 86_399_000),
            NOW,
        );
        assert_eq!(history.live.session_state(NOW, "qwen", None), "idle");
        assert_eq!(
            history.live.turns.back().unwrap().outcome.as_deref(),
            Some("completed")
        );
    }

    #[test]
    fn delayed_started_phase_cannot_reopen_a_terminal_prompt_but_a_new_retry_can() {
        for outcome in ["completed", "cancelled", "error"] {
            let mut log = log();
            log.own_id("s");
            observe_at(&mut log, &attempt("p", NOW), NOW);
            observe_at(&mut log, &result("p", outcome, NOW + 10), NOW + 10);
            let mut late = request("p", "started", NOW);
            late["timestamp"] = crate::iso(NOW + 20);
            observe_at(&mut log, &late, NOW + 20);
            assert!(
                !log.live.turns.back().unwrap().running,
                "late {outcome} phase"
            );
            assert_eq!(log.live.turns.back().unwrap().observed, NOW + 10);

            let mut retry = request("p", "started", NOW + 30);
            retry["timestamp"] = crate::iso(NOW + 30);
            retry["systemPayload"]["uiEvent"]["startedAt"] = json!(NOW + 30);
            observe_at(&mut log, &retry, NOW + 30);
            assert!(
                log.live.turns.back().unwrap().running,
                "new retry after {outcome}"
            );
            assert_eq!(log.live.turns.back().unwrap().observed, NOW + 30);
            assert!(log.live.turns.back().unwrap().outcome.is_none());
        }
    }

    #[test]
    fn delayed_terminal_prompt_phase_cannot_replace_a_newer_prompt() {
        let mut log = log();
        log.own_id("s");
        observe_at(&mut log, &attempt("old", NOW), NOW);
        observe_at(&mut log, &result("old", "cancelled", NOW + 10), NOW + 10);
        observe_at(&mut log, &attempt("new", NOW + 20), NOW + 20);
        let mut late = request("old", "started", NOW);
        late["timestamp"] = crate::iso(NOW + 30);
        observe_at(&mut log, &late, NOW + 30);
        assert_eq!(log.live.turns.back().unwrap().id.as_deref(), Some("new"));
        assert!(log.live.turns.back().unwrap().running);
        assert_eq!(log.live.turns.back().unwrap().observed, NOW + 20);
    }
}
