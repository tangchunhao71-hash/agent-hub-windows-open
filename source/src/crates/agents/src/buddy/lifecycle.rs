// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! The request lifecycle written by both Buddy clients, independent of usage.
//! A completed assistant message can precede another tool in the same request;
//! it makes the mark idle but is not proof of a successful completed user turn.
use crate::transcript::live::{Activity, Call, Work};
use crate::transcript::{at, text};
use crate::Log;
use serde_json::{json, Value};

const MAX_FUTURE_MS: i64 = 90_000;

#[derive(Clone, Copy)]
enum Event<'a> {
    Prompt,
    Working,
    Call(&'a str, &'a str),
    Result(&'a str, bool),
    End(Option<&'static str>),
}

fn event(v: &Value) -> Option<Event<'_>> {
    match text(&v["type"])? {
        "message" if v["role"] == "user" => (v["providerData"]["startsNewUserRequest"] == true
            || (v["providerData"]["isMeta"] != true
                && v["providerData"]["startsNewUserRequest"] != false))
            .then_some(Event::Prompt),
        "reasoning" => Some(Event::Working),
        "function_call" => Some(Event::Call(text(&v["callId"])?, text(&v["name"])?)),
        "function_call_result" => Some(Event::Result(
            text(&v["callId"])?,
            matches!(
                text(&v["status"]),
                Some("failed" | "error" | "cancelled" | "canceled")
            ),
        )),
        "message" if v["role"] == "assistant" => match text(&v["status"]) {
            Some("completed") => Some(Event::End(None)),
            Some("cancelled" | "canceled" | "interrupted") => Some(Event::End(Some("interrupted"))),
            Some("failed" | "error") => Some(Event::End(Some("failed"))),
            Some("incomplete") => Some(Event::End(
                if v["providerData"]["error"]["message"] == "Request was aborted." {
                    Some("interrupted")
                } else {
                    (!v["providerData"]["error"].is_null()).then_some("failed")
                },
            )),
            Some("pending" | "streaming" | "in_progress") => Some(Event::Working),
            _ => None,
        },
        _ => None,
    }
}

fn work(name: &str) -> Option<Work> {
    match name {
        "Bash" | "PowerShell" | "Shell" => Some(Work::Shell),
        "Read" | "Glob" | "Grep" | "WebFetch" | "WebSearch" => Some(Work::Read),
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => Some(Work::Edit),
        "Task" | "Agent" => Some(Work::Agents),
        "AskUserQuestion" | "ExitPlanMode" => Some(Work::Question),
        _ => None,
    }
}

pub(super) fn observe(log: &mut Log, v: &Value) {
    let Some(event) = event(v) else { return };
    let Some(at) = at(&v["timestamp"]).filter(|at| {
        *at > 0
            && *at
                <= chrono::Utc::now()
                    .timestamp_millis()
                    .saturating_add(MAX_FUTURE_MS)
    }) else {
        return;
    };
    let previous = &log.source_state["buddyLifecycle"];
    let previous_request = text(&previous["request"]).map(str::to_owned);
    let explicit_request = text(&v["providerData"]["conversationRequestId"]);
    let request = explicit_request
        .or_else(|| {
            matches!(event, Event::Prompt)
                .then(|| text(&v["id"]))
                .flatten()
        })
        .or(previous_request.as_deref())
        .map(str::to_owned);
    let Some(request) = request else { return };
    let prompt = matches!(event, Event::Prompt);
    let same_request = previous_request.as_deref() == Some(request.as_str());
    // A late event from an older request cannot replace a newer prompt. Only a
    // real user prompt opens another request; metadata and tool-result users do not.
    if previous["at"].as_i64().is_some_and(|last| at < last)
        || (previous_request.is_some() && !same_request && !prompt)
        || (same_request && (prompt || previous["terminal"] == true))
    {
        return;
    }
    // A tool result without its open call is historical/partial evidence, not
    // permission to restart an otherwise idle request (including equal-clock replay).
    if let Event::Result(id, _) = event {
        if !same_request
            || !log
                .live
                .activity
                .as_ref()
                .is_some_and(|activity| activity.calls.iter().any(|call| call.id == id))
        {
            return;
        }
    }
    let started = if same_request {
        previous["started"].as_i64()
    } else if prompt {
        Some(at)
    } else {
        None
    };
    let (running, outcome) = match event {
        Event::End(outcome) => (false, outcome),
        _ => (true, None),
    };
    let terminal = matches!(event, Event::End(_)) && v["status"] != "completed";
    // Source outcomes deliberately never include "completed": only the verified
    // Stop hook may establish successful completion and generate its reminder.
    log.live
        .observe_source(&request, running, started, at, outcome);
    if prompt {
        log.live
            .activity
            .get_or_insert_with(Activity::default)
            .start();
    }
    if running {
        let activity = log.live.activity.get_or_insert_with(Activity::default);
        match event {
            Event::Call(id, name) => activity.open(Call {
                id: id.into(),
                work: work(name),
                at,
                request: matches!(name, "AskUserQuestion" | "ExitPlanMode")
                    .then(|| "question".into()),
            }),
            Event::Result(id, failed) => activity.close(id, at, failed),
            Event::Working => activity.line = Some("response".into()),
            _ => {}
        }
    }
    log.source_state["buddyLifecycle"] = json!({
        "request":request,"started":started,"at":at,"terminal":terminal
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_905_100_000;

    fn log(source: &'static str) -> Log {
        Log::new("session.jsonl".into(), source)
    }
    fn record(kind: &str, role: &str, status: Option<&str>, request: &str, at: i64) -> Value {
        json!({"id":format!("{kind}-{at}"),"type":kind,"role":role,"status":status,
            "timestamp":at,"sessionId":"session","providerData":{"conversationRequestId":request}})
    }
    fn prompt(request: &str, at: i64) -> Value {
        record("message", "user", None, request, at)
    }
    fn state(log: &Log, at: i64) -> &'static str {
        log.live.session_state(at, &log.source, None)
    }

    #[test]
    fn both_buddies_run_from_prompt_and_finish_without_fabricated_success() {
        for source in ["workbuddy", "codebuddy"] {
            let mut log = log(source);
            let mut user = prompt("r1", NOW);
            if source == "workbuddy" {
                user["providerData"]["startsNewUserRequest"] = json!(true);
            }
            log.parse(&user);
            assert_eq!(state(&log, NOW + 1), "running");
            log.parse(&record("reasoning", "", None, "r1", NOW + 10));
            assert_eq!(state(&log, NOW + 11), "running");
            log.parse(&record(
                "message",
                "assistant",
                Some("completed"),
                "r1",
                NOW + 20,
            ));
            assert_eq!(state(&log, NOW + 21), "idle");
            assert_eq!(log.live.turns.len(), 1);
            assert_eq!(log.live.turns.back().unwrap().outcome, None);
        }
    }

    #[test]
    fn an_assistant_text_before_a_tool_does_not_finish_the_whole_request() {
        let mut log = log("workbuddy");
        log.parse(&prompt("r1", NOW));
        log.parse(&record(
            "message",
            "assistant",
            Some("completed"),
            "r1",
            NOW + 10,
        ));
        let mut call = record("function_call", "", None, "r1", NOW + 11);
        call["callId"] = json!("c1");
        call["name"] = json!("Read");
        log.parse(&call);
        assert_eq!(state(&log, NOW + 12), "running");
        assert_eq!(log.live.turns.len(), 1);
        assert_eq!(log.live.turns.back().unwrap().started, Some(NOW));
        let mut result = record(
            "function_call_result",
            "",
            Some("completed"),
            "r1",
            NOW + 13,
        );
        result["callId"] = json!("c1");
        log.parse(&result);
        assert_eq!(state(&log, NOW + 14), "running");
        assert!(log.live.activity.as_ref().unwrap().calls.is_empty());
    }

    #[test]
    fn a_question_waits_until_its_result_and_never_grants_permission() {
        let mut log = log("codebuddy");
        log.parse(&prompt("r1", NOW));
        let mut call = record("function_call", "", None, "r1", NOW + 1);
        call["callId"] = json!("ask");
        call["name"] = json!("AskUserQuestion");
        log.parse(&call);
        assert_eq!(state(&log, NOW + 2), "waiting");
        let mut result = record("function_call_result", "", Some("completed"), "r1", NOW + 3);
        result["callId"] = json!("ask");
        log.parse(&result);
        assert_eq!(state(&log, NOW + 4), "running");
    }

    #[test]
    fn orphan_or_replayed_results_do_not_create_activity() {
        let mut log = log("codebuddy");
        let mut result = record("function_call_result", "", Some("completed"), "r1", NOW);
        result["callId"] = json!("missing");
        log.parse(&result);
        assert_eq!(state(&log, NOW + 1), "idle");
        log.parse(&prompt("r1", NOW + 1));
        let end = record("message", "assistant", Some("completed"), "r1", NOW + 2);
        log.parse(&end);
        result["timestamp"] = json!(NOW + 2);
        log.parse(&result);
        assert_eq!(state(&log, NOW + 3), "idle");
    }

    #[test]
    fn errors_and_cancellation_end_work_and_reject_late_same_request_activity() {
        for status in [
            "incomplete",
            "failed",
            "error",
            "cancelled",
            "canceled",
            "interrupted",
        ] {
            let mut log = log("workbuddy");
            log.parse(&prompt("r1", NOW));
            let mut end = record("message", "assistant", Some(status), "r1", NOW + 1);
            if status == "incomplete" {
                end["providerData"]["error"] = json!({"status":429});
            }
            log.parse(&end);
            assert_eq!(state(&log, NOW + 2), "idle", "{status}");
            let outcome = if matches!(status, "cancelled" | "canceled" | "interrupted") {
                "interrupted"
            } else {
                "failed"
            };
            assert_eq!(
                log.live.turns.back().unwrap().outcome.as_deref(),
                Some(outcome)
            );
            log.parse(&record("reasoning", "", None, "r1", NOW + 3));
            assert_eq!(state(&log, NOW + 4), "idle", "{status}");
            log.parse(&prompt("r2", NOW + 5));
            assert_eq!(state(&log, NOW + 6), "running");
        }
    }

    #[test]
    fn internal_user_metadata_and_unknown_records_do_not_restart_a_request() {
        let mut log = log("codebuddy");
        for key in ["isMeta", "startsNewUserRequest"] {
            let mut user = prompt("r1", NOW);
            user["providerData"][key] = json!(key == "isMeta");
            log.parse(&user);
            assert_eq!(state(&log, NOW + 1), "idle");
        }
        log.parse(&record("ai-title", "", None, "r1", NOW + 1));
        assert_eq!(state(&log, NOW + 2), "idle");
    }

    #[test]
    fn explicit_new_request_starts_a_scheduled_meta_prompt() {
        let mut log = log("workbuddy");
        let mut user = prompt("r1", NOW);
        user["providerData"]["isMeta"] = json!(true);
        user["providerData"]["startsNewUserRequest"] = json!(true);
        log.parse(&user);
        assert_eq!(state(&log, NOW + 1), "running");
    }

    #[test]
    fn client_abort_notice_ends_as_interrupted() {
        let mut log = log("workbuddy");
        log.parse(&prompt("r1", NOW));
        let mut end = record("message", "assistant", Some("incomplete"), "r1", NOW + 1);
        end["providerData"]["error"] = json!({"message":"Request was aborted."});
        log.parse(&end);
        assert_eq!(state(&log, NOW + 2), "idle");
        assert_eq!(
            log.live.turns.back().unwrap().outcome.as_deref(),
            Some("interrupted")
        );
    }

    #[test]
    fn incomplete_without_an_error_is_terminal_but_does_not_claim_failure_or_success() {
        let mut log = log("codebuddy");
        log.parse(&prompt("r1", NOW));
        log.parse(&record(
            "message",
            "assistant",
            Some("incomplete"),
            "r1",
            NOW + 1,
        ));
        log.parse(&record("reasoning", "", None, "r1", NOW + 2));
        assert_eq!(state(&log, NOW + 3), "idle");
        assert_eq!(log.live.turns.back().unwrap().outcome, None);
    }

    #[test]
    fn missing_bad_and_far_future_timestamps_never_create_work() {
        let mut log = log("workbuddy");
        for timestamp in [Value::Null, json!("invalid"), json!(-5), json!(i64::MAX)] {
            let mut user = prompt("r1", NOW);
            user["timestamp"] = timestamp;
            log.parse(&user);
        }
        assert!(log.live.turns.is_empty());
        assert_eq!(state(&log, NOW), "idle");
    }

    #[test]
    fn rfc3339_timestamps_and_old_unclosed_requests_use_the_existing_expiry() {
        let mut log = log("codebuddy");
        let mut user = prompt("r1", NOW);
        user["timestamp"] = json!(chrono::DateTime::from_timestamp_millis(NOW)
            .unwrap()
            .to_rfc3339());
        log.parse(&user);
        assert_eq!(state(&log, NOW + 1), "running");
        assert_eq!(state(&log, NOW + 1_800_001), "idle");
        assert_eq!(
            log.live.next_change(NOW + 1, "codebuddy", None),
            Some(NOW + 1_800_000)
        );
    }

    #[test]
    fn rereads_and_out_of_order_requests_cannot_reopen_finished_or_newer_work() {
        let mut log = log("workbuddy");
        let user = prompt("r1", NOW);
        let end = record("message", "assistant", Some("completed"), "r1", NOW + 10);
        log.parse(&user);
        log.parse(&end);
        log.parse(&user);
        log.parse(&end);
        assert_eq!(state(&log, NOW + 11), "idle");
        assert_eq!(log.live.turns.len(), 1);
        log.parse(&prompt("r2", NOW + 20));
        log.parse(&record(
            "message",
            "assistant",
            Some("completed"),
            "r1",
            NOW + 30,
        ));
        assert_eq!(state(&log, NOW + 31), "running");
        assert_eq!(log.live.turns.back().unwrap().id.as_deref(), Some("r2"));
    }

    #[test]
    fn lifecycle_records_leave_metering_deduplication_intact() {
        let mut log = log("workbuddy");
        log.parse(&prompt("r1", NOW));
        let mut end = record("message", "assistant", Some("completed"), "r1", NOW + 1);
        end["providerData"]["messageId"] = json!("m1");
        end["providerData"]["model"] = json!("test-model");
        end["providerData"]["usage"] = json!({"input_tokens":10,"output_tokens":20});
        log.parse(&end);
        log.parse(&end);
        assert_eq!(log.usage.input, 10);
        assert_eq!(log.usage.output, 20);
        assert_eq!(log.messages.len(), 1);
        assert_eq!(state(&log, NOW + 2), "idle");
    }

    #[test]
    fn checkpoint_round_trip_retains_the_request_clock_and_stops_replayed_prompts() {
        let mut log = log("codebuddy");
        let user = prompt("r1", NOW);
        log.parse(&user);
        log.parse(&record(
            "message",
            "assistant",
            Some("completed"),
            "r1",
            NOW + 1,
        ));
        let mut restored: Log =
            serde_json::from_value(serde_json::to_value(&log).unwrap()).unwrap();
        restored.parse(&user);
        assert_eq!(state(&restored, NOW + 2), "idle");
        assert_eq!(restored.live.turns.len(), 1);
    }
}
