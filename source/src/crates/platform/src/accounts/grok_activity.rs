//! Short-lived read-only live-state requests, independent of the quota gate.
use super::AccountContext;
use serde_json::Value;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, Weak};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct GrokActivityReader {
    context: AccountContext,
    identities: Arc<Mutex<Option<(Instant, Vec<Value>)>>>,
}

impl GrokActivityReader {
    pub(super) fn new(context: &AccountContext) -> Self {
        let mut context = context.clone();
        // Stopping this reader must not stop the account-collection worker.
        context.cancelled = Arc::new(AtomicBool::new(false));
        context.children = Arc::new(Mutex::new(Vec::new()));
        Self { context, identities: Arc::new(Mutex::new(None)) }
    }
    pub fn read(&self) -> Result<Vec<Value>, String> {
        if self.context.cancelled.load(Ordering::Acquire) {
            return Err(agent_hud_core::l10n::text("工作状态读取已停止", "Activity reading was stopped"));
        }
        let result = agent_hud_agents::grok::activity::read(&self.context);
        if self.context.cancelled.load(Ordering::Acquire) {
            return Err(agent_hud_core::l10n::text("工作状态读取已停止", "Activity reading was stopped"));
        }
        let mut rows = result?;
        let mut cached = self.identities.lock().unwrap_or_else(|p| p.into_inner());
        let reusable = cached.as_ref().is_some_and(|(at, identities)| at.elapsed() < Duration::from_secs(30)
            && rows.iter().all(|row| identities.iter().any(|identity| identity["id"] == row["id"])));
        if !reusable {
            *cached = agent_hud_agents::grok::identity::read(&self.context).ok().map(|rows| (Instant::now(),rows));
        }
        if let Some((_, identities)) = cached.as_ref() {
            for row in &mut rows {
                if let Some(identity) = identities.iter().find(|identity| identity["id"] == row["id"]) {
                    let avatar = &identity["avatar"];
                    // Photos travel only for visible working/finished Bots. Idle
                    // rows retain their names and small generated fallback.
                    let avatar = if avatar["kind"] == "photo" && !matches!(row["state"].as_str(),Some("running"|"waiting"))
                        && row.get("settlementId").is_none() {
                        serde_json::json!({"kind":"shape","shape":avatar["fallback"]["shape"],"color":avatar["fallback"]["color"]})
                    } else { avatar.clone() };
                    row["botIdentity"] = serde_json::json!({"name":identity["name"],"avatar":avatar});
                }
            }
        }
        Ok(rows)
    }
    /// Cancels any in-flight native helper; cancellation is permanent.
    pub fn stop(&self) {
        self.context.cancelled.store(true, Ordering::Release);
        *self.identities.lock().unwrap_or_else(|p|p.into_inner()) = None;
        let children = self.context.children.lock().map(|children| {
            children.iter().filter_map(Weak::upgrade).collect::<Vec<_>>()
        }).unwrap_or_default();
        for child in children {
            if let Ok(mut child) = child.lock() { let _ = child.kill(); }
        }
    }
}
