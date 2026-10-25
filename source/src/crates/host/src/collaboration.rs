// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Bounded, opt-in Codex <-> CodeBuddy text delegation using existing CLI logins.
//! The desktop owns grants; a helper cannot grant itself permission or delegate.
mod adapter;
#[cfg(test)]
mod tests;

use crate::extension::{HostContext, ProductExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Endpoint {
    program: PathBuf,
    #[serde(default)]
    script: Option<PathBuf>,
    login_file: PathBuf,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    schema_version: u32,
    private_directory: PathBuf,
    endpoints: BTreeMap<String, Endpoint>,
    #[serde(default)]
    initial_sessions: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ledger {
    schema_version: u32,
    bindings: BTreeMap<String, String>,
    tasks: BTreeMap<String, Value>,
}

pub struct Collaboration {
    config: Config,
    ledger: Mutex<Ledger>,
    host: Mutex<Option<HostContext>>,
    stopped: AtomicBool,
    faulted: AtomicBool,
}

fn client(value: &str) -> bool {
    matches!(value, "codex" | "codebuddy")
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn valid_session(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn owner_key(source: &str, session: &str) -> String {
    format!("{source}/{session}")
}
fn active(task: &Value) -> bool {
    matches!(task["state"].as_str(), Some("queued" | "running"))
}

impl Collaboration {
    pub fn from_env() -> Result<Option<Arc<Self>>, String> {
        // Explicit local experiment only; a leftover config cannot enable it in
        // the release product. Installers never include credentials or grants.
        if std::env::var("AGENTBOT_LOCAL_EXPERIMENTS").as_deref() != Ok("1") {
            return Ok(None);
        }
        let file = match std::env::var_os("AGENTHUD_COLLABORATION_CONFIG") {
            Some(file) => PathBuf::from(file),
            None => {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let file = exe
                    .parent()
                    .ok_or("Missing application directory")?
                    .join("collaboration/config.json");
                if !file.is_file() {
                    return Ok(None);
                }
                file
            }
        };
        Self::from_file(&file).map(Some)
    }

    fn from_file(file: &Path) -> Result<Arc<Self>, String> {
        let config: Config = serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Invalid collaboration configuration: {e}"))?;
        if config.schema_version != 2 || !config.private_directory.is_absolute() {
            return Err(
                "Collaboration requires schemaVersion 2 and an absolute privateDirectory".into(),
            );
        }
        for (source, endpoint) in &config.endpoints {
            if !client(source)
                || !endpoint.program.is_absolute()
                || !endpoint.program.is_file()
                || !endpoint.login_file.is_absolute()
                || endpoint
                    .script
                    .as_ref()
                    .is_some_and(|p| !p.is_absolute() || !p.is_file())
                || (source == "codebuddy" && endpoint.script.is_none())
            {
                return Err(
                    "Invalid CLI endpoint; use verified absolute executable/login paths".into(),
                );
            }
        }
        fs::create_dir_all(&config.private_directory).map_err(|e| e.to_string())?;
        let path = config.private_directory.join("tasks-v2.json");
        let mut ledger: Ledger = if path.exists() {
            serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Invalid task ledger; refusing to replace it: {e}"))?
        } else {
            Ledger {
                schema_version: 2,
                ..Ledger::default()
            }
        };
        if ledger.schema_version != 2 {
            return Err("Unsupported task ledger".into());
        }
        for task in ledger.tasks.values_mut() {
            if active(task) {
                task["state"] = json!("deliveryUnknown");
                task["note"] = json!("Host restarted. Not replayed; inspect the original receipt.");
            }
        }
        // Initial grants only seed a NEW ledger. Reopening cannot undo a UI revocation.
        if !path.exists() {
            for (source, sessions) in &config.initial_sessions {
                let endpoint = config
                    .endpoints
                    .get(source)
                    .ok_or("Initial session endpoint missing")?;
                let scope = adapter::login_scope(source, &endpoint.login_file)?;
                for session in sessions {
                    if !valid_session(session) {
                        return Err("Invalid initial session id".into());
                    }
                    ledger
                        .bindings
                        .insert(owner_key(source, session), scope.clone());
                }
            }
        }
        let product = Arc::new(Self {
            config,
            ledger: Mutex::new(ledger),
            host: Mutex::new(None),
            stopped: AtomicBool::new(false),
            faulted: AtomicBool::new(false),
        });
        product.mutate(|_| Ok(()))?;
        Ok(product)
    }

    // All ledger mutations and durable writes share one lock. A failed write restores
    // memory and disables dispatch, so an unpersisted task can never start.
    fn mutate<T>(
        &self,
        change: impl FnOnce(&mut Ledger) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut ledger = self.ledger.lock().map_err(|_| "Task ledger unavailable")?;
        let previous = ledger.clone();
        let result = match change(&mut ledger) {
            Ok(value) => value,
            Err(e) => {
                *ledger = previous;
                return Err(e);
            }
        };
        let save = (|| {
            let pending = self.config.private_directory.join("tasks-v2.pending.json");
            fs::write(
                &pending,
                serde_json::to_vec_pretty(&*ledger).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            fs::rename(pending, self.config.private_directory.join("tasks-v2.json"))
                .map_err(|e| e.to_string())
        })();
        if let Err(error) = save {
            *ledger = previous;
            self.faulted.store(true, Ordering::Release);
            return Err(format!(
                "Receipt persistence failed; dispatch stopped: {error}"
            ));
        }
        Ok(result)
    }

    fn scope(&self, source: &str) -> Result<String, String> {
        let endpoint = self
            .config
            .endpoints
            .get(source)
            .ok_or("CLI endpoint is not configured")?;
        adapter::login_scope(source, &endpoint.login_file)
    }
    fn owner(&self, caller: &Value) -> Result<(String, String), String> {
        let source = caller["client"]
            .as_str()
            .filter(|s| client(s))
            .ok_or("Codex or CodeBuddy caller required")?;
        let session = caller["sessionId"]
            .as_str()
            .filter(|s| valid_session(s))
            .ok_or("Exact caller session required")?;
        let key = owner_key(source, session);
        let scope = self.scope(source)?;
        let ledger = self.ledger.lock().map_err(|_| "Task ledger unavailable")?;
        if ledger.bindings.get(&key) != Some(&scope) {
            return Err(
                "This session/account is not authorized. Bind the session in Agent Hub settings."
                    .into(),
            );
        }
        if session.starts_with("ah-helper-")
            || ledger
                .tasks
                .values()
                .any(|t| t["helperSessionId"] == session)
        {
            return Err("Helpers cannot delegate; maximum delegation depth is one".into());
        }
        Ok((key, scope))
    }
    fn task(&self, id: &str) -> Result<Value, String> {
        self.ledger
            .lock()
            .map_err(|_| "Task ledger unavailable")?
            .tasks
            .get(id)
            .cloned()
            .ok_or("Unknown task".into())
    }
    fn finish(
        &self,
        id: &str,
        state: &str,
        result: Value,
        session: Option<String>,
    ) -> Result<(), String> {
        self.mutate(|ledger| {
            let task = ledger.tasks.get_mut(id).ok_or("Unknown task")?;
            task["state"] = json!(state);
            task["result"] = result;
            task["finishedAt"] = json!(now());
            if let Some(session) = session {
                task["helperSessionId"] = json!(session);
            }
            Ok(())
        })
    }
    fn scopes_match(&self, task: &Value) -> bool {
        [("source", "ownerScope"), ("target", "helperScope")]
            .iter()
            .all(|(source, scope)| {
                task[*source]
                    .as_str()
                    .and_then(|s| self.scope(s).ok())
                    .is_some_and(|s| task[*scope] == s)
            })
    }
    fn cancelled(&self, id: &str) -> bool {
        self.stopped.load(Ordering::Acquire)
            || self.faulted.load(Ordering::Acquire)
            || self
                .task(id)
                .map(|t| t["cancelRequested"] == true)
                .unwrap_or(true)
    }
    fn execute(&self, id: &str) -> Result<(), String> {
        if self.cancelled(id) {
            return Ok(());
        }
        let task = self.mutate(|ledger| {
            let task = ledger.tasks.get_mut(id).ok_or("Unknown task")?;
            if task["state"] != "queued" {
                return Ok(None);
            }
            task["state"] = json!("running");
            task["startedAt"] = json!(now());
            Ok(Some(task.clone()))
        })?;
        let Some(task) = task else {
            return Ok(());
        };
        if !self.scopes_match(&task) || self.cancelled(id) {
            return self.finish(
                id,
                "failed",
                json!("Not delivered: login changed or cancelled before delivery"),
                None,
            );
        }
        let target = task["target"].as_str().ok_or("Missing target")?;
        let endpoint = self
            .config
            .endpoints
            .get(target)
            .ok_or("Missing endpoint")?;
        let outcome = adapter::execute(
            target,
            endpoint,
            &self.config.private_directory,
            &task,
            || self.cancelled(id) || !self.scopes_match(&task),
        );
        self.finish(id, outcome.state, json!(outcome.message), outcome.session)
    }
    fn cancel(&self, id: &str) -> Result<Value, String> {
        self.mutate(|ledger| {
            let task = ledger.tasks.get_mut(id).ok_or("Unknown task")?;
            if task["state"] == "queued" {
                task["state"] = json!("cancelled");
                task["finishedAt"] = json!(now());
            }
            if task["state"] == "running" {
                task["cancelRequested"] = json!(true);
            }
            Ok(task.clone())
        })
    }
    fn tool(&self, params: Value) -> Result<Value, String> {
        let (owner, owner_scope) = self.owner(&params["caller"])?;
        let name = params["name"].as_str().ok_or("Missing tool name")?;
        if name == "collaboration_list" {
            let tasks: Vec<Value> = self
                .ledger
                .lock()
                .map_err(|_| "Task ledger unavailable")?
                .tasks
                .values()
                .filter(|t| t["owner"] == owner && t["ownerScope"] == owner_scope)
                .cloned()
                .collect();
            return Ok(json!({"tasks":tasks}));
        }
        let args = &params["arguments"];
        let id = args["taskId"]
            .as_str()
            .filter(|s| valid_id(s))
            .ok_or("taskId must be 1-80 ASCII letters/digits/hyphens")?;
        if let Ok(previous) = self.task(id) {
            if previous["owner"] != owner || previous["ownerScope"] != owner_scope {
                return Err("Task belongs to another session/account".into());
            }
        }
        match name {
            "collaboration_submit" => {
                let source = params["caller"]["client"]
                    .as_str()
                    .ok_or("Missing client")?;
                let target = args["target"]
                    .as_str()
                    .filter(|s| client(s) && *s != source)
                    .ok_or("Target must be the other agent: codex or codebuddy")?;
                let prompt = args["prompt"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty() && s.len() <= 8192)
                    .ok_or("Prompt must be 1-8192 UTF-8 bytes")?;
                if args.get("parentTaskId").is_some() || args.get("depth").is_some() {
                    return Err("Nested delegation is disabled".into());
                }
                // Duplicates return the SAME terminal receipt, even if the helper logged out.
                if let Ok(previous) = self.task(id) {
                    if previous["prompt"] != prompt || previous["target"] != target {
                        return Err(
                            "Task ID is already bound to different content or target".into()
                        );
                    }
                    return Ok(previous);
                }
                if self.stopped.load(Ordering::Acquire) || self.faulted.load(Ordering::Acquire) {
                    return Err("Dispatch is stopped".into());
                }
                let helper_scope = self.scope(target)?;
                self.mutate(|ledger| {
                    if ledger.tasks.contains_key(id) { return Err("Concurrent task submission; read the existing receipt".into()); }
                    if ledger.tasks.len() >= 128 { return Err("Preview ledger full (128); retain receipts before a new ledger".into()); }
                    let task = json!({"taskId":id,"sequence":ledger.tasks.len(),"source":source,"target":target,
                        "owner":owner,"ownerScope":owner_scope,"helperScope":helper_scope,
                        "helperSessionId":format!("ah-helper-{id}"),"prompt":prompt,"state":"queued",
                        "cancelRequested":false,"createdAt":now(),"depth":1,"timeoutSeconds":120});
                    ledger.tasks.insert(id.to_owned(), task.clone());
                    Ok(task)
                })
            }
            "collaboration_status" => self.task(id),
            "collaboration_cancel" => self.cancel(id),
            _ => Err("Unknown collaboration tool".into()),
        }
    }
    fn overview(&self) -> Result<Value, String> {
        let ledger = self.ledger.lock().map_err(|_| "Task ledger unavailable")?;
        let endpoints: Vec<Value> = self
            .config
            .endpoints
            .keys()
            .map(|source| match self.scope(source) {
                Ok(_) => json!({"id":source,"ready":true,"authentication":"existing-login"}),
                Err(note) => json!({"id":source,"ready":false,"note":note}),
            })
            .collect();
        let bindings: Vec<Value> = ledger.bindings.iter().map(|(key, scope)| {
            let (source, session) = key.split_once('/').unwrap_or(("", ""));
            json!({"client":source,"sessionId":session,"valid":self.scope(source).ok().as_ref()==Some(scope)})
        }).collect();
        Ok(
            json!({"enabled":!self.stopped.load(Ordering::Acquire)&&!self.faulted.load(Ordering::Acquire),
            "endpoints":endpoints,"bindings":bindings,"tasks":ledger.tasks.values().collect::<Vec<_>>(),"maxDepth":1}),
        )
    }
    // Only the native Settings route reaches these methods; the MCP channel permits
    // host_tools / host_tool only. Binding always requires an explicit desktop action.
    fn bind(&self, params: &Value) -> Result<Value, String> {
        let source = params["client"]
            .as_str()
            .filter(|s| client(s))
            .ok_or("Invalid client")?;
        let session = params["sessionId"]
            .as_str()
            .filter(|s| valid_session(s) && !s.starts_with("ah-helper-"))
            .ok_or("Invalid main session")?;
        let key = owner_key(source, session);
        let enabled = params["enabled"]
            .as_bool()
            .ok_or("enabled must be boolean")?;
        let scope = if enabled {
            Some(self.scope(source)?)
        } else {
            None
        };
        self.mutate(|ledger| {
            if ledger
                .tasks
                .values()
                .any(|t| t["helperSessionId"] == session)
            {
                return Err("A helper cannot be a dispatching session".into());
            }
            if let Some(scope) = scope {
                ledger.bindings.insert(key.clone(), scope);
            } else {
                ledger.bindings.remove(&key);
                for task in ledger.tasks.values_mut().filter(|t| t["owner"] == key) {
                    if task["state"] == "queued" {
                        task["state"] = json!("cancelled");
                    }
                    if task["state"] == "running" {
                        task["cancelRequested"] = json!(true);
                    }
                }
            }
            Ok(())
        })?;
        self.overview()
    }
    pub fn run_worker(self: &Arc<Self>) {
        let owner = Arc::clone(self);
        std::thread::spawn(move || {
            while !owner.stopped.load(Ordering::Acquire) && !owner.faulted.load(Ordering::Acquire) {
                let id = owner.ledger.lock().ok().and_then(|ledger| {
                    ledger
                        .tasks
                        .iter()
                        .filter(|(_, t)| t["state"] == "queued")
                        .min_by_key(|(_, t)| t["sequence"].as_u64().unwrap_or(u64::MAX))
                        .map(|(id, _)| id.clone())
                });
                if let Some(id) = id {
                    if let Err(error) = owner.execute(&id) {
                        let _ = owner.finish(&id, "deliveryUnknown", json!(error), None);
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
    }
}

impl ProductExtension for Collaboration {
    fn start(&self, host: HostContext) -> Result<(), String> {
        *self.host.lock().map_err(|_| "Host unavailable")? = Some(host);
        Ok(())
    }
    fn host_tools_on(&self) -> bool {
        true
    }
    fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "host_tools" => Ok(json!({"tools":[
                {"name":"collaboration_submit","description":"Delegate one user-authorized text-only task to the other agent using its existing login. Max depth 1, timeout 120s. Use a stable taskId. Never replay deliveryUnknown. Read the final receipt and review the result for the user.","readOnly":false,"inputSchema":{"type":"object","additionalProperties":false,"properties":{"taskId":{"type":"string"},"target":{"enum":["codex","codebuddy"]},"prompt":{"type":"string"}},"required":["taskId","target","prompt"]}},
                {"name":"collaboration_status","description":"Read your task receipt and returned helper text. Completed is helper completion; the caller must still review it.","readOnly":true,"inputSchema":{"type":"object","properties":{"taskId":{"type":"string"}},"required":["taskId"]}},
                {"name":"collaboration_list","description":"Read only tasks owned by this caller session/account.","readOnly":true,"inputSchema":{"type":"object","properties":{}}},
                {"name":"collaboration_cancel","description":"Cancel queued work or stop the local helper. Remote cancellation is not guaranteed; running work becomes deliveryUnknown.","readOnly":false,"inputSchema":{"type":"object","properties":{"taskId":{"type":"string"}},"required":["taskId"]}}
            ]})),
            "host_tool" => self.tool(params),
            "collaboration_overview" => self.overview(),
            "collaboration_bind" => self.bind(&params),
            "collaboration_submit_ui" => self.tool(json!({"name":"collaboration_submit",
                "caller":{"client":params["client"],"sessionId":params["sessionId"]},
                "arguments":{"taskId":params["taskId"],"target":params["target"],"prompt":params["prompt"]}})),
            "collaboration_cancel_ui" => self.cancel(params["taskId"].as_str().ok_or("Missing taskId")?),
            _ => Err("Unknown collaboration method".into()),
        }
    }
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }
}
