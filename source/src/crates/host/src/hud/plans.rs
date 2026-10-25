//! Each screen's HUD window: its size and place, from the screen, the screen's
//! display preferences. The transparent canvas covers the complete monitor.
//! The app plans its own windows, so Settings can close.
use super::{
    configure_display, display_catalog, reconcile, Display, Edge, HudConfig, HudMonitor, Mode,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

fn defaults() -> &'static Value {
    static DEFAULTS: OnceLock<Value> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../packages/protocol/hud-defaults.json"
        ))
        .expect("checked-in HUD defaults")
    })
}

/// A screen's display preferences as its HUD draws them (use-hud-config.ts
/// `getScreenHUDConfig`): the screen's own, else the screens' defaults, else the
/// app's; a value of the wrong type is no value.
pub(crate) struct Screen<'a> {
    view: &'a Value,
    id: &'a str,
    has_notch: bool,
}

impl<'a> Screen<'a> {
    /// `view` is the presentation owner's view, drafts included.
    pub(crate) fn new(view: &'a Value, id: &'a str, has_notch: bool) -> Self {
        Self {
            view,
            id,
            has_notch,
        }
    }

    fn value(&self, key: &str) -> Value {
        let fallback = &defaults()[key];
        let fits = |value: &&Value| match fallback {
            Value::Number(_) => value.as_f64().is_some_and(f64::is_finite),
            Value::Bool(_) => value.is_boolean(),
            Value::String(_) => value.is_string(),
            _ => false,
        };
        let store = &self.view["store"];
        [
            &store["screens"][self.id][key],
            &store["screenDefaults"][key],
        ]
        .into_iter()
        .find(fits)
        .cloned()
        .unwrap_or_else(|| match key {
            "mode" => json!(if self.has_notch { "notch" } else { "logos" }),
            _ => fallback.clone(),
        })
    }

    fn number(&self, key: &str) -> f64 {
        self.value(key).as_f64().unwrap_or(0.0)
    }

    pub(crate) fn text(&self, key: &str) -> String {
        self.value(key).as_str().unwrap_or("").to_owned()
    }

    pub(crate) fn mode(&self) -> Mode {
        serde_json::from_value(self.value("mode")).unwrap_or(if self.has_notch {
            Mode::Notch
        } else {
            Mode::Logos
        })
    }

    pub(crate) fn mode_name(&self) -> &'static str {
        match self.mode() {
            Mode::Notch => "notch",
            Mode::Logos => "logos",
            Mode::Simple => "simple",
        }
    }

    /// The panel layout the screen draws: its own, else the shared one.
    pub(crate) fn layout(&self) -> &'a Value {
        match &self.view["layouts"][self.id] {
            Value::Null => &self.view["sharedLayout"],
            layout => layout,
        }
    }
}

/// Each screen has its own full-monitor transparent canvas. Only visible HUD
/// regions receive input; pets can reach the real screen edges without moving
/// the dock or changing the saved placement offset.
fn plan(monitor: &HudMonitor, view: &Value, visible: bool) -> HudConfig {
    let screen = Screen::new(view, &monitor.screen_id, monitor.notch.is_some());
    let mode = screen.mode();
    let notch = mode == Mode::Notch;
    let edge = if notch {
        Edge::Top
    } else {
        serde_json::from_value(screen.value("edge")).unwrap_or_default()
    };
    let scale = monitor.scale_factor;
    HudConfig {
        mode,
        edge,
        offset: if notch { 0.5 } else { screen.number("offset") },
        width: f64::from(monitor.bounds.width) / scale,
        height: f64::from(monitor.bounds.height) / scale,
        ignore_cursor: true,
        visible,
        monitor_index: monitor.index,
        screen_id: Some(monitor.screen_id.clone()),
    }
}

/// The connected screens and the plan each one's window was last given.
#[derive(Default)]
pub(crate) struct Planner {
    displays: Vec<Display>,
    measured: String,
    configured: HashMap<String, String>,
}

impl Planner {
    /// Measures the displays again; returns them when they changed. The owners
    /// of disconnected screens are ended first.
    pub(crate) fn measure(
        &mut self,
        app: &tauri::AppHandle,
    ) -> Result<Option<Vec<HudMonitor>>, String> {
        let displays = display_catalog(app)?;
        let monitors: Vec<HudMonitor> = displays
            .iter()
            .map(|display| display.info.clone())
            .collect();
        let signature = serde_json::to_string(&monitors).map_err(|error| error.to_string())?;
        self.displays = displays;
        if signature == self.measured {
            return Ok(None);
        }
        let connected: HashSet<String> = monitors
            .iter()
            .map(|monitor| monitor.screen_id.clone())
            .collect();
        reconcile(app, &connected);
        self.configured.retain(|id, _| connected.contains(id));
        self.measured = signature;
        Ok(Some(monitors))
    }

    /// The screens as the presentation owner and its MCP tools name them.
    pub(crate) fn screens(&self) -> Vec<Value> {
        self.displays
            .iter()
            .filter_map(|display| {
                let mut value = serde_json::to_value(&display.info).ok()?;
                value["id"] = json!(display.info.screen_id);
                value["hasNotch"] = json!(display.info.notch.is_some());
                Some(value)
            })
            .collect()
    }

    /// The primary screen's id and whether it has a notch.
    pub(crate) fn primary(&self) -> Option<(&str, bool)> {
        self.displays
            .iter()
            .find(|display| display.info.primary)
            .or(self.displays.first())
            .map(|display| {
                (
                    display.info.screen_id.as_str(),
                    display.info.notch.is_some(),
                )
            })
    }

    /// Gives each screen's window its plan where the plan changed.
    pub(crate) fn apply(
        &mut self,
        app: &tauri::AppHandle,
        view: &Value,
        snapshot: Option<&Value>,
        _now_ms: i64,
    ) {
        let visible = snapshot.is_none_or(|snapshot| snapshot["settings"]["hudVisible"] != false);
        for display in &self.displays {
            let config = plan(&display.info, view, visible);
            let id = display.info.screen_id.clone();
            let Ok(signature) =
                serde_json::to_string(&(&config, display.info.bounds, display.info.scale_factor))
            else {
                continue;
            };
            if self.configured.get(&id) == Some(&signature) {
                continue;
            }
            match configure_display(app, config, display.clone()) {
                Ok(()) => {
                    self.configured.insert(id, signature);
                }
                Err(error) => eprintln!("Agent Bot HUD window: {error}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(screen: Value, layout: Value) -> Value {
        json!({"store": {"screens": {"a": screen}, "screenDefaults": {}}, "layouts": {"a": layout}, "sharedLayout": {}})
    }

    #[test]
    fn a_screen_reads_its_own_value_then_the_defaults() {
        let shared = json!({"store": {"screens": {}, "screenDefaults": {"edge": "left"}}, "layouts": {}, "sharedLayout": {"style": {"width": 600}}});
        let screen = Screen::new(&shared, "a", false);
        assert!(screen.mode() == Mode::Logos);
        assert!(Screen::new(&shared, "a", true).mode() == Mode::Notch);
        assert_eq!(screen.text("edge"), "left");
        assert_eq!(screen.layout()["style"]["width"], 600);
        let own = view(
            json!({"mode": "simple", "offset": "far", "logoSize": 30}),
            json!({"hud": {}}),
        );
        let screen = Screen::new(&own, "a", true);
        assert!(screen.mode() == Mode::Simple);
        // A value of the wrong type falls back to the default.
        assert_eq!(screen.number("offset"), 0.5);
        assert_eq!(screen.number("logoSize"), 30.0);
        assert_eq!(screen.layout(), &json!({"hud": {}}));
    }

    #[test]
    fn pet_canvas_covers_each_full_monitor_for_every_dock_and_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for (x, y, width, height) in [
                (0, 0, 1920, 1080),
                (-2560, -1440, 2560, 1440),
                (1920, 0, 800, 600),
            ] {
                let bounds = super::super::Rect { x, y, width, height };
                let monitor = HudMonitor {
                    screen_id: "a".into(),
                    window_label: "hud".into(),
                    index: 1,
                    runtime_id: "a".into(),
                    persistent_id: None,
                    name: None,
                    primary: false,
                    bounds,
                    work_area: bounds,
                    scale_factor: scale,
                    notch: None,
                    notch_coordinate_space: "global-logical-top-left",
                };
                for mode in ["logos", "simple", "notch"] {
                    for edge in ["top", "bottom", "left", "right"] {
                        for offset in [0.0, 0.3, 1.0] {
                            let value = view(
                                json!({"mode": mode, "edge": edge, "offset": offset}),
                                json!({"style": {"width": 540}}),
                            );
                            let config = plan(&monitor, &value, true);
                            assert_eq!(config.width, f64::from(width) / scale);
                            assert_eq!(config.height, f64::from(height) / scale);
                            assert_eq!(config.offset, if mode == "notch" { 0.5 } else { offset });
                            let frame = super::super::edge_frame(bounds, scale, &config);
                            assert_eq!(
                                (frame.x, frame.y, frame.width, frame.height),
                                (x, y, width, height),
                            );
                        }
                    }
                }
            }
        }
    }
}
