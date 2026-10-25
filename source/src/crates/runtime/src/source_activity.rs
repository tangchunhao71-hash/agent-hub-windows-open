//! Local run-status polling stays off the collector/UI lock and never shares
//! the slow billing cadence. Disabled generations cannot publish late results.
use serde_json::Value;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

type Wake = Arc<dyn Fn() + Send + Sync>;
struct State {
    enabled: bool,
    stopped: bool,
    generation: u64,
    rows: Vec<Value>,
    wake: Wake,
}
impl State {
    fn publish(&mut self, generation: u64, rows: Vec<Value>) -> Option<Wake> {
        if self.stopped || !self.enabled || self.generation != generation || self.rows == rows {
            return None;
        }
        self.rows = rows;
        Some(self.wake.clone())
    }
}
pub(super) struct Activity {
    state: Arc<(Mutex<State>, Condvar)>,
}
impl Activity {
    pub fn start(enabled: bool) -> Self {
        let state = Arc::new((Mutex::new(State {
            enabled, stopped: false, generation: 0, rows: vec![], wake: Arc::new(|| {}),
        }), Condvar::new()));
        let owned = state.clone();
        std::thread::spawn(move || loop {
            let generation = {
                let mut guard = owned.0.lock().unwrap_or_else(|p| p.into_inner());
                while !guard.stopped && !guard.enabled {
                    guard = owned.1.wait(guard).unwrap_or_else(|p| p.into_inner());
                }
                if guard.stopped { return; }
                guard.generation
            };
            let rows = agent_hud_platform::clients::antigravity::activity::read().unwrap_or_default();
            let wake = owned.0.lock().unwrap_or_else(|p| p.into_inner()).publish(generation, rows);
            if let Some(wake) = wake { wake(); }
            let guard = owned.0.lock().unwrap_or_else(|p| p.into_inner());
            drop(owned.1.wait_timeout_while(guard, Duration::from_secs(2),
                |s| !s.stopped && s.enabled && s.generation == generation));
        });
        Self { state }
    }
    pub fn set_waker(&self, wake: Wake) {
        self.state.0.lock().unwrap_or_else(|p| p.into_inner()).wake = wake;
    }
    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.0.lock().unwrap_or_else(|p| p.into_inner());
        if state.stopped || state.enabled == enabled { return; }
        state.enabled = enabled;
        state.rows.clear();
        state.generation = state.generation.wrapping_add(1);
        self.state.1.notify_all();
    }
    pub fn rows(&self) -> Option<Vec<Value>> {
        let state = self.state.0.lock().unwrap_or_else(|p| p.into_inner());
        (state.enabled && !state.stopped).then(|| state.rows.clone())
    }
    pub fn stop(&self) {
        let mut state = self.state.0.lock().unwrap_or_else(|p| p.into_inner());
        state.stopped = true;
        state.rows.clear();
        self.state.1.notify_all();
    }
}
impl Drop for Activity {
    fn drop(&mut self) { self.stop(); }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_failure_disable_reenable_and_stop_never_resurrect_work() {
        let mut state=State {enabled:true,stopped:false,generation:3,rows:vec![],wake:Arc::new(|| {})};
        let running=vec![serde_json::json!({"state":"running"})];
        assert!(state.publish(2,running.clone()).is_none());
        assert!(state.publish(3,running.clone()).is_some());
        assert!(state.publish(3,vec![]).is_some());
        assert!(state.rows.is_empty());
        state.enabled=false;
        assert!(state.publish(3,running.clone()).is_none());
        state.enabled=true;state.generation=4;
        assert!(state.publish(3,running.clone()).is_none());
        assert!(state.publish(4,running.clone()).is_some());
        state.stopped=true;state.rows.clear();
        assert!(state.publish(4,running).is_none());
    }
}
