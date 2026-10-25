use super::super::desktop_io::IpcError;
use agent_hud_agents::codex::AGENT;
use agent_hud_core::l10n;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub(super) const ELICITATION_METHOD: &str =
    "thread-follower-submit-mcp-server-elicitation-response";
pub(super) const COMMAND_METHOD: &str = "thread-follower-command-approval-decision";
pub(super) const PERMISSIONS_METHOD: &str = "thread-follower-permissions-request-approval-response";
const QUESTION_METHOD: &str = "thread-follower-steer-turn";
#[derive(Clone)]
pub(super) struct Pending {
    pub id: String,
    pub thread: String,
    pub owner: String,
    pub wire: Value,
    pub request_id: Value,
    pub view: Value,
    pub scopes: Vec<String>,
    pub turn_id: Option<String>,
    pub question: Option<(String, Vec<String>)>,
}

fn identity(prefix: &str, thread: &str, owner: &str, wire: &Value) -> String {
    format!(
        "{prefix}:{:x}",
        Sha256::digest(serde_json::to_vec(&json!([thread, owner, wire])).unwrap_or_default())
    )
}
/// The website a browser approval is for.
fn origin_of(metadata: &Value) -> Option<&str> {
    text(&metadata["origin"]).or_else(|| text(&metadata["tool_params"]["origin"]))
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|v| !v.is_empty())
}
fn scopes(value: &Value) -> Option<Vec<String>> {
    let values = match value {
        Value::Null => vec![],
        Value::String(value) => vec![value.as_str()],
        Value::Array(values) => values
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    if values.iter().any(|v| !matches!(*v, "session" | "always")) {
        return None;
    }
    Some(values.into_iter().map(str::to_owned).collect())
}
fn view(thread: &str, summary: &str, tool: &str, cwd: &Value, kind: &str) -> Value {
    json!({"source":AGENT.id,"vendor":AGENT.name,"sessionId":thread,"title":summary,"summary":summary,"toolName":tool,"kind":kind,"input":"","detail":null,"cwd":cwd,"createdAt":chrono::Utc::now().to_rfc3339(),"allowsAlways":false,"questions":[],"answersQuestion":kind=="question","answersPlan":false,"plan":null,"suggestions":[]})
}

fn allows_native_approval(permissions: &Value, granular_kind: &str) -> bool {
    if permissions["approvalsReviewer"] != "user" { return false; }
    let policy = &permissions["approvalPolicy"];
    matches!(policy.as_str(), Some("on-request" | "on-failure" | "untrusted"))
        || policy["granular"][granular_kind] == true
}

/// Verified against Codex Desktop 26.1021.3705.0 main-BPBEjAxi.js. Only a
/// command's one-shot accept/decline is exposed; policy/session changes stay
/// in the Desktop. The owner must still hold this exact request on a live turn.
fn command_pending(state: &Value, wire: &Value, thread: &str, owner: &str) -> Result<Option<Pending>, IpcError> {
    let params = &wire["params"];
    if wire["method"] != "item/commandExecution/requestApproval"
        || wire["completed"] == true || params["threadId"] != thread
    { return Ok(None); }
    let request_id = &wire["id"];
    if request_id.as_str().is_none_or(str::is_empty)
        && !request_id.is_i64() && !request_id.is_u64()
    { return Ok(None); }
    let (Some(turn_id), Some(_item_id), Some(command)) =
        (text(&params["turnId"]), text(&params["itemId"]), params["command"].as_str().filter(|command| !command.trim().is_empty()))
    else { return Ok(None); };
    if current_turn(state)? != Some(turn_id) { return Ok(None); }
    let turns = turns(state, true)?;
    let Some(turn) = turns.into_iter().find(|turn| turn["turnId"] == turn_id && turn["status"] == "inProgress")
    else { return Ok(None); };
    let permissions = &turn["params"];
    if !allows_native_approval(permissions, "sandbox_approval") {
        return Ok(None);
    }
    let mut card = view(thread, &l10n::text("允许执行此命令？", "Allow this command?"),
        "exec_command", &params["cwd"], "permission");
    card["input"] = json!(command);
    card["detail"] = json!(text(&params["reason"]));
    card["allowLabel"] = json!(l10n::text("批准一次", "Allow once"));
    card["badge"] = json!("CMD");
    Ok(Some(Pending {id:identity("codex-desktop-command",thread,owner,wire),thread:thread.into(),
        owner:owner.into(),wire:wire.clone(),request_id:request_id.clone(),view:card,
        scopes:vec![],turn_id:Some(turn_id.into()),question:None}))
}

/// The current client models fileSystem.entries as {access,path:{type,path}}.
/// Deliberately expose only explicit absolute paths: network, glob patterns,
/// special roots and unknown fields cannot be granted by an incomplete card.
fn explicit_file_permissions(value: &Value) -> Option<Value> {
    let permissions = value.as_object()?;
    if permissions.keys().any(|key| !matches!(key.as_str(), "fileSystem" | "network"))
        || permissions.get("network").is_some_and(|network| !network.is_null())
    { return None; }
    let file_system = permissions.get("fileSystem")?.as_object()?;
    if file_system.keys().any(|key| key != "entries") { return None; }
    let entries = file_system.get("entries")?.as_array()?;
    if entries.is_empty() || entries.len() > 256 { return None; }
    for entry in entries {
        let object = entry.as_object()?;
        if object.keys().any(|key| !matches!(key.as_str(), "access" | "path"))
            || !matches!(entry["access"].as_str(), Some("read" | "write"))
        { return None; }
        let path = entry["path"].as_object()?;
        if path.keys().any(|key| !matches!(key.as_str(), "type" | "path"))
            || entry["path"]["type"] != "path"
        { return None; }
        let path = entry["path"]["path"].as_str()?;
        if path.len() > 4096 || path.chars().any(char::is_control)
            || !std::path::Path::new(path).is_absolute()
        { return None; }
    }
    Some(json!({"fileSystem":value["fileSystem"]}))
}

fn permissions_pending(state: &Value, wire: &Value, thread: &str, owner: &str) -> Result<Option<Pending>, IpcError> {
    let params = &wire["params"];
    if wire["method"] != "item/permissions/requestApproval" || wire["completed"] == true
        || params["threadId"] != thread
    { return Ok(None); }
    let request_id = &wire["id"];
    if request_id.as_str().is_none_or(str::is_empty) && !request_id.is_i64() && !request_id.is_u64() {
        return Ok(None);
    }
    let Some(turn_id) = text(&params["turnId"]) else { return Ok(None); };
    if current_turn(state)? != Some(turn_id) { return Ok(None); }
    let turns = turns(state, true)?;
    let Some(turn) = turns.into_iter().find(|turn| turn["turnId"] == turn_id && turn["status"] == "inProgress")
    else { return Ok(None); };
    if !allows_native_approval(&turn["params"], "request_permissions") {
        return Ok(None);
    }
    let Some(permissions) = explicit_file_permissions(&params["permissions"]) else { return Ok(None); };
    let mut card = view(thread, &l10n::text("允许本回合访问这些文件？", "Allow these files for this turn?"),
        "request_permissions", &state["cwd"], "permission");
    let entries = permissions["fileSystem"]["entries"].as_array().ok_or(IpcError::Protocol)?;
    let lines = entries.iter().map(|entry| {
        let access = if entry["access"] == "read" {l10n::text("读取", "Read")} else {l10n::text("写入", "Write")};
        format!("{access}: {}", entry["path"]["path"].as_str().unwrap())
    }).collect::<Vec<_>>().join("\n");
    card["input"] = json!(lines);
    let scope = l10n::text("仅当前回合；不会授予会话或永久权限。", "This turn only; no session or permanent permission.");
    card["detail"] = json!(text(&params["reason"]).map_or(scope.clone(), |reason| format!("{scope}\n\n{reason}")));
    card["allowLabel"] = json!(l10n::text("允许本回合", "Allow this turn"));
    card["badge"] = json!("FILES");
    Ok(Some(Pending {id:identity("codex-desktop-files",thread,owner,wire),thread:thread.into(),
        owner:owner.into(),wire:wire.clone(),request_id:request_id.clone(),view:card,
        scopes:vec![],turn_id:Some(turn_id.into()),question:None}))
}

pub(super) fn parse(state: &Value, thread: &str, owner: &str) -> Result<Vec<Pending>, IpcError> {
    let requests = state["requests"].as_array().ok_or(IpcError::Protocol)?;
    let mut found = Vec::new();
    for wire in requests {
        if let Some(command) = command_pending(state, wire, thread, owner)? {
            found.push(command);
            continue;
        }
        if let Some(permissions) = permissions_pending(state, wire, thread, owner)? {
            found.push(permissions);
            continue;
        }
        let params = &wire["params"];
        let metadata = &params["_meta"];
        if wire["method"] != "mcpServer/elicitation/request"
            || wire["completed"] == true
            || params["threadId"] != thread
            || params["mode"] != "form"
            || metadata["codex_approval_kind"] != "mcp_tool_call"
        {
            continue;
        }
        let request_id = &wire["id"];
        if request_id.as_str().is_none_or(|v| v.is_empty())
            && !request_id.is_i64()
            && !request_id.is_u64()
        {
            continue;
        }
        let Some(scopes) = scopes(&metadata["persist"]) else {
            continue;
        };
        let (summary, tool, subject): (String, String, Option<String>) =
            match metadata["connector_id"].as_str() {
                Some("computer-use") => {
                    let Some(app) = text(&metadata["tool_params"]["app"]) else {
                        continue;
                    };
                    if !scopes.iter().any(|v| v == "session") {
                        continue;
                    }
                    let display = metadata["tool_params_display"]
                        .as_array()
                        .and_then(|values| values.iter().find(|value| value["name"] == "app"))
                        .and_then(|value| text(&value["value"]))
                        .unwrap_or(app);
                    (
                        if l10n::chinese() {
                            format!("允许 ChatGPT 使用 {display}？")
                        } else {
                            format!("Allow ChatGPT to use {display}?")
                        },
                        "computer_use_app".into(),
                        Some(app.to_owned()),
                    )
                }
                Some("browser-use") => {
                    let turn = params["turnId"]
                        .as_str()
                        .map(|id| {
                            turns(state, true).map(|values| {
                                values.into_iter().rev().find(|turn| turn["turnId"] == id)
                            })
                        })
                        .transpose()?
                        .flatten();
                    // The turn's own settings; a turn without them shows no
                    // card. Only a request no turn is found for falls back on
                    // the thread's current permissions (agent-hud-native-open).
                    let settings = match turn {
                        Some(turn) => &turn["params"],
                        None => &state["currentPermissions"],
                    };
                    if settings["approvalsReviewer"] != "user" {
                        continue;
                    }
                    let tool = metadata["tool_name"].as_str().unwrap_or("");
                    let opens = matches!(
                        tool,
                        "access_browser_origin" | "access_browser_origin_with_raw_cdp"
                    );
                    if opens
                        && settings["approvalPolicy"] == "never"
                        && settings["sandboxPolicy"]["type"] == "dangerFullAccess"
                    {
                        continue;
                    }
                    let origin = origin_of(metadata);
                    let name = text(&metadata["connector_name"]).unwrap_or("ChatGPT");
                    let summary =
                        if let Some(origin) = origin.filter(|_| tool == "access_browser_origin") {
                            if l10n::chinese() {
                                format!("允许 {name} 访问 {origin}？")
                            } else {
                                format!("Allow {name} to access {origin}?")
                            }
                        } else if let Some(message) = text(&params["message"]) {
                            message.into()
                        } else {
                            continue;
                        };
                    (
                        summary,
                        if tool.is_empty() {
                            "browser".into()
                        } else {
                            tool.into()
                        },
                        origin.map(str::to_owned),
                    )
                }
                _ => continue,
            };
        let mut card = view(thread, &summary, &tool, &state["cwd"], "permission");
        let detail = [
            (metadata["riskLevel"] == "high").then(|| l10n::text("较高风险", "Elevated Risk")),
            text(&metadata["subtitle"]).map(str::to_owned),
            subject,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n");
        card["detail"] = json!(detail);
        card["input"] = card["detail"].clone();
        // What the card offers, from Core as Open's PermissionRequest words
        // it: always allowing a website adds the site itself, and an app has
        // no rule to show; the allow button keeps the answer for the
        // conversation where the request persists it per session.
        let always = scopes.iter().any(|v| v == "always");
        let website = tool != "computer_use_app";
        card["allowsAlways"] = json!(always);
        card["alwaysAllowRule"] = if always && website {
            json!(origin_of(metadata))
        } else {
            Value::Null
        };
        card["allowLabel"] = json!(if website && !scopes.iter().any(|v| v == "session") {
            l10n::text("批准一次", "Allow once")
        } else {
            l10n::text("允许此对话", "Allow this conversation")
        });
        card["badge"] = json!(if website { "WEB" } else { "APP" });
        let id = identity("codex-desktop-permission", thread, owner, wire);
        found.push(Pending {
            id,
            thread: thread.into(),
            owner: owner.into(),
            wire: wire.clone(),
            request_id: request_id.clone(),
            view: card,
            scopes,
            turn_id: None,
            question: None,
        });
    }
    let turn_store = turns(state, true)?;
    if let Some(turn) = turn_store
        .iter()
        .rev()
        .find(|turn| turn["status"] == "inProgress")
    {
        let Some(turn_id) = text(&turn["turnId"]) else {
            return Ok(found);
        };
        let items = turn["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let answered = items.iter().flat_map(answered_ids).collect::<HashSet<_>>();
        for item in items {
            if item["type"] != "agentMessage" {
                continue;
            }
            let Some(source) = text(&item["id"]) else {
                continue;
            };
            let Some(questions) = item["questions"].as_array() else {
                continue;
            };
            for (index, question) in questions.iter().enumerate() {
                let Some(title) = text(&question["title"]) else {
                    continue;
                };
                let labels = if question["options"].is_null() {
                    Vec::new()
                } else {
                    let Some(labels) = question["options"].as_array().and_then(|values| {
                        values
                            .iter()
                            .map(|v| v.as_str().map(str::to_owned))
                            .collect::<Option<Vec<_>>>()
                    }) else {
                        continue;
                    };
                    labels
                };
                let key =
                    serde_json::to_string(&json!(["request_user_input_async", source, index]))
                        .map_err(|_| IpcError::Protocol)?;
                if answered.contains(&key) {
                    continue;
                }
                let wire = json!({"turnId":turn_id,"sourceItemId":source,"questionIndex":index,"question":question});
                let mut card = view(
                    thread,
                    title,
                    "request_user_input_async",
                    &state["cwd"],
                    "question",
                );
                card["questions"] = json!([{"id":key,"answerKey":key,"question":title,"options":labels.iter().map(|label|json!({"label":label})).collect::<Vec<_>>(),"multiSelect":false}]);
                found.push(Pending {
                    id: identity("codex-desktop-question", thread, owner, &wire),
                    thread: thread.into(),
                    owner: owner.into(),
                    wire,
                    request_id: json!(key),
                    view: card,
                    scopes: vec![],
                    turn_id: Some(turn_id.into()),
                    question: Some((title.into(), labels)),
                });
            }
        }
    }
    Ok(found)
}

pub(super) fn turns(state: &Value, include_overlay: bool) -> Result<Vec<&Value>, IpcError> {
    let overlay = match state.get("turns") {
        None => vec![],
        Some(value) => value
            .as_array()
            .ok_or(IpcError::Protocol)?
            .iter()
            .collect::<Vec<_>>(),
    };
    if overlay.iter().any(|turn| !turn.is_object()) {
        return Err(IpcError::Protocol);
    }
    let Some(storage) = state.get("turnHistory").filter(|v| !v.is_null()) else {
        return Ok(overlay);
    };
    if !storage.is_object() {
        return Err(IpcError::Protocol);
    }
    if storage["kind"] != "canonical" {
        return Ok(overlay);
    }
    let history = &storage["history"];
    let islands = history["islands"].as_array().ok_or(IpcError::Protocol)?;
    let entities = history["entitiesByKey"]
        .as_object()
        .ok_or(IpcError::Protocol)?;
    let mut canonical = Vec::new();
    for island in islands {
        for entry in island["entries"].as_array().ok_or(IpcError::Protocol)? {
            let key = entry["value"].as_str().ok_or(IpcError::Protocol)?;
            let turn = entities
                .get(key)
                .filter(|v| v.is_object())
                .ok_or(IpcError::Protocol)?;
            canonical.push(turn);
        }
    }
    let ids = canonical
        .iter()
        .filter_map(|turn| turn["turnId"].as_str())
        .collect::<HashSet<_>>();
    if overlay
        .iter()
        .any(|turn| turn["turnId"].as_str().is_some_and(|id| ids.contains(id)))
    {
        return Err(IpcError::Protocol);
    }
    if include_overlay {
        canonical.extend(overlay);
    }
    Ok(canonical)
}
fn current_turn(state: &Value) -> Result<Option<&str>, IpcError> {
    Ok(turns(state, false)?
        .into_iter()
        .rev()
        .find(|turn| {
            !turn["turnId"].is_null()
                || !matches!(turn["status"].as_str(), Some("completed" | "failed"))
        })
        .and_then(|turn| turn["turnId"].as_str()))
}
fn answered_ids(item: &Value) -> Vec<String> {
    let content = match item["type"].as_str() {
        Some("userMessage") => &item["content"],
        Some("steeringUserMessage") if item["status"] == "accepted" => &item["input"],
        _ => return vec![],
    };
    let Some(parts) = content
        .as_array()
        .filter(|v| v.len() == 1 && v[0]["type"] == "text")
    else {
        return vec![];
    };
    let Some(text) = parts[0]["text"].as_str() else {
        return vec![];
    };
    let Some(payload) = text
        .trim()
        .strip_prefix("<send_user_message_question_reply>")
        .and_then(|v| v.strip_suffix("</send_user_message_question_reply>"))
    else {
        return vec![];
    };
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return vec![];
    };
    let answers = if let Some(values) = value.as_array().filter(|v| !v.is_empty()) {
        values.clone()
    } else if value.is_object() {
        vec![value]
    } else {
        return vec![];
    };
    if answers.iter().any(|v| {
        !v["questionItemId"].is_string() || !v["question"].is_string() || !v["answer"].is_string()
    }) {
        return vec![];
    }
    answers
        .into_iter()
        .filter_map(|v| v["questionItemId"].as_str().map(str::to_owned))
        .collect()
}
pub(super) fn answer(
    item: &Pending,
    decision: &Value,
    state: &Value,
) -> Result<(&'static str, Value), IpcError> {
    let kind = decision["kind"].as_str().ok_or(IpcError::Protocol)?;
    if kind == "leave" {
        return Ok((ELICITATION_METHOD, Value::Null));
    }
    if item.wire["method"] == "item/commandExecution/requestApproval" {
        let present = state["requests"].as_array().is_some_and(|requests| requests.iter().any(|wire| wire == &item.wire));
        if !present || command_pending(state, &item.wire, &item.thread, &item.owner)?.is_none() {
            return Err(IpcError::Protocol);
        }
        return match kind {
            "allow" => Ok((COMMAND_METHOD, json!("accept"))),
            "deny" => Ok((COMMAND_METHOD, json!("decline"))),
            _ => Err(IpcError::Protocol),
        };
    }
    if item.wire["method"] == "item/permissions/requestApproval" {
        let present = state["requests"].as_array().is_some_and(|requests| requests.iter().any(|wire| wire == &item.wire));
        if !present || permissions_pending(state, &item.wire, &item.thread, &item.owner)?.is_none() {
            return Err(IpcError::Protocol);
        }
        let permissions = match kind {
            "allow" => explicit_file_permissions(&item.wire["params"]["permissions"]).ok_or(IpcError::Protocol)?,
            "deny" => json!({}),
            _ => return Err(IpcError::Protocol),
        };
        return Ok((PERMISSIONS_METHOD, json!({"permissions":permissions,"scope":"turn"})));
    }
    if let Some((question, labels)) = &item.question {
        if kind != "answer" || current_turn(state)? != item.turn_id.as_deref() {
            return Err(IpcError::Protocol);
        }
        let key = item.request_id.as_str().ok_or(IpcError::Protocol)?;
        let answers = decision["answers"].as_object().ok_or(IpcError::Protocol)?;
        if answers.keys().any(|v| v != key) {
            return Err(IpcError::Protocol);
        }
        let Some(answer) = answers.get(key) else {
            return Ok((QUESTION_METHOD, Value::Null));
        };
        let selected = match answer.get("selected") {
            None => &[][..],
            Some(value) => value.as_array().ok_or(IpcError::Protocol)?.as_slice(),
        };
        let mut words = selected
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|v| labels.iter().any(|label| label == v))
                    .map(str::to_owned)
                    .ok_or(IpcError::Protocol)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(custom) = answer.get("custom").filter(|v| !v.is_null()) {
            words.push(custom.as_str().ok_or(IpcError::Protocol)?.into());
        }
        let text = words.join(", ");
        let text = text.trim();
        if text.is_empty() {
            return Err(IpcError::Protocol);
        }
        let encoded = serde_json::to_string(
            &json!([{"questionItemId":key,"question":question,"answer":text}]),
        )
        .map_err(|_| IpcError::Protocol)?;
        return Ok((QUESTION_METHOD,json!(format!("<send_user_message_question_reply>\n{encoded}\n</send_user_message_question_reply>"))));
    }
    let response = match kind {
        "allow" if item.scopes.iter().any(|v| v == "session") => {
            json!({"action":"accept","content":{},"_meta":{"persist":"session"}})
        }
        "allow" if item.view["toolName"] != "computer_use_app" => {
            json!({"action":"accept","content":{},"_meta":null})
        }
        "always" if item.scopes.iter().any(|v| v == "always") => {
            json!({"action":"accept","content":{},"_meta":{"persist":"always"}})
        }
        "deny" => json!({"action":"decline","content":null,"_meta":null}),
        _ => return Err(IpcError::Protocol),
    };
    Ok((ELICITATION_METHOD, response))
}
