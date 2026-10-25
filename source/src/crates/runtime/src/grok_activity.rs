//! Read-only Bot status has its own cadence; slow billing jobs cannot hold it up.
use agent_hud_platform::GrokActivityReader;
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
        if self.stopped || !self.enabled || self.generation != generation { return None; }
        if self.rows == rows { return None; }
        self.rows = rows; Some(self.wake.clone())
    }
}
pub(super) struct Activity {
    state: Arc<(Mutex<State>, Condvar)>,
    reader: GrokActivityReader,
}
impl Activity {
    pub fn start(reader: GrokActivityReader, enabled: bool) -> Self {
        let state = Arc::new((Mutex::new(State { enabled, stopped:false, generation:0, rows:vec![], wake:Arc::new(||{}) }),Condvar::new()));
        let owner = Self { state:state.clone(), reader:reader.clone() };
        std::thread::spawn(move || loop {
            let generation = {
                let mut state_guard = state.0.lock().unwrap_or_else(|p|p.into_inner());
                while !state_guard.stopped && !state_guard.enabled {
                    state_guard = state.1.wait(state_guard).unwrap_or_else(|p|p.into_inner());
                }
                if state_guard.stopped { return; }
                state_guard.generation
            };
            // Errors remove positive state immediately. Core also expires successful
            // reports independently, including while an HTTP request is stalled.
            let rows = reader.read().unwrap_or_default();
            let wake = state.0.lock().unwrap_or_else(|p|p.into_inner()).publish(generation,rows);
            if let Some(wake) = wake { wake(); }
            let guard = state.0.lock().unwrap_or_else(|p|p.into_inner());
            drop(state.1.wait_timeout_while(guard,Duration::from_secs(5),|s|!s.stopped && s.enabled && s.generation==generation));
        });
        owner
    }
    pub fn set_waker(&self, wake: Wake) { self.state.0.lock().unwrap_or_else(|p|p.into_inner()).wake = wake; }
    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.0.lock().unwrap_or_else(|p|p.into_inner());
        if state.enabled == enabled || state.stopped { return; }
        state.enabled=enabled; state.rows.clear(); state.generation=state.generation.wrapping_add(1);
        self.state.1.notify_all();
    }
    pub fn rows(&self) -> Option<Vec<Value>> {
        let state = self.state.0.lock().unwrap_or_else(|p|p.into_inner());
        (state.enabled && !state.stopped).then(||state.rows.clone())
    }
    pub fn stop(&self) {
        { let mut state=self.state.0.lock().unwrap_or_else(|p|p.into_inner());state.stopped=true;state.rows.clear();self.state.1.notify_all(); }
        self.reader.stop();
    }
}
impl Drop for Activity { fn drop(&mut self) { self.stop(); } }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_or_superseded_requests_never_publish_stale_work() {
        let mut state=State{enabled:true,stopped:false,generation:1,rows:vec![],wake:Arc::new(||{})};
        let running=vec![serde_json::json!({"state":"running"})];
        assert!(state.publish(0,running.clone()).is_none());assert!(state.rows.is_empty());
        assert!(state.publish(1,running.clone()).is_some());
        state.enabled=false;state.rows.clear();assert!(state.publish(1,running.clone()).is_none());
        state.enabled=true;state.generation=2;assert!(state.publish(1,running.clone()).is_none());
        assert!(state.publish(2,running).is_some());
        assert!(state.publish(2,vec![]).is_some());assert!(state.rows.is_empty());
        state.stopped=true;assert!(state.publish(2,vec![Value::Null]).is_none());
    }
}
