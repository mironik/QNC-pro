//! The mouse map. A gesture on a panel is the actions `contracts/input/mouse.json` binds to it
//! (one or several, in order), with the frame under the pointer as their value; the rule is
//! the user's: left click sets the playhead, right click is Ctrl+ (takes the nearer thing),
//! the wheel is the arrows, Ctrl+wheel takes and moves. The map is built in and checked
//! against the action catalog once per process. Pure.

use std::collections::BTreeMap;
use std::sync::OnceLock;

pub const MAP: &str = include_str!("../../../contracts/input/mouse.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Gesture {
    LeftClick,
    RightClick,
    DoubleClick,
    MiddleClick,
    Drag,
    WheelBack,
    WheelForward,
    CtrlWheelBack,
    CtrlWheelForward,
}

impl Gesture {
    pub const ALL: [Gesture; 9] = [
        Gesture::LeftClick,
        Gesture::RightClick,
        Gesture::DoubleClick,
        Gesture::MiddleClick,
        Gesture::Drag,
        Gesture::WheelBack,
        Gesture::WheelForward,
        Gesture::CtrlWheelBack,
        Gesture::CtrlWheelForward,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Gesture::LeftClick => "left_click",
            Gesture::RightClick => "right_click",
            Gesture::DoubleClick => "double_click",
            Gesture::MiddleClick => "middle_click",
            Gesture::Drag => "drag",
            Gesture::WheelBack => "wheel_back",
            Gesture::WheelForward => "wheel_forward",
            Gesture::CtrlWheelBack => "ctrl_wheel_back",
            Gesture::CtrlWheelForward => "ctrl_wheel_forward",
        }
    }

    fn of_name(name: &str) -> Option<Gesture> {
        Gesture::ALL.into_iter().find(|g| g.name() == name)
    }
}

#[derive(Debug)]
pub struct MouseMap {
    /// panel -> gesture -> action ids
    panels: BTreeMap<String, BTreeMap<Gesture, Vec<String>>>,
}

impl MouseMap {
    pub fn parse(json: &str) -> Result<MouseMap, String> {
        let raw: BTreeMap<String, BTreeMap<String, Vec<String>>> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let catalog = qnc_actions::catalog();
        let mut panels = BTreeMap::new();
        for (panel, gestures) in raw {
            let mut bound = BTreeMap::new();
            for (name, actions) in gestures {
                let gesture = Gesture::of_name(&name).ok_or_else(|| format!("mouse map: no gesture is called {name}"))?;
                for action in &actions {
                    if !catalog.has(action) {
                        return Err(qnc_text::fill("mouse.unknown_action", &[("gesture", &name), ("panel", &panel), ("action", action)]));
                    }
                }
                bound.insert(gesture, actions);
            }
            panels.insert(panel, bound);
        }
        Ok(MouseMap { panels })
    }

    /// The actions of a gesture on a panel, in order; none when the map does not bind it.
    pub fn actions(&self, panel: &str, gesture: Gesture) -> &[String] {
        self.panels.get(panel).and_then(|g| g.get(&gesture)).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn panels(&self) -> impl Iterator<Item = &str> {
        self.panels.keys().map(String::as_str)
    }
}

static BUILT_IN: OnceLock<MouseMap> = OnceLock::new();

/// The built-in mouse map.
pub fn map() -> &'static MouseMap {
    BUILT_IN.get_or_init(|| MouseMap::parse(MAP).expect("the built-in mouse map is valid; its test says so"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_map_parses_and_an_unknown_action_or_gesture_is_refused() {
        let map = MouseMap::parse(MAP).unwrap();
        assert_eq!(map.actions("source_timeline", Gesture::RightClick), ["timeline_take_nearer"]);
        assert_eq!(map.actions("source_timeline", Gesture::CtrlWheelBack), ["timeline_take_nearer", "step_back_frame"]);
        assert!(map.actions("nowhere", Gesture::LeftClick).is_empty());
        assert!(MouseMap::parse(r#"{"p":{"left_click":["nothing"]}}"#).unwrap_err().contains("nothing"));
        assert!(MouseMap::parse(r#"{"p":{"tap":["mark_in"]}}"#).unwrap_err().contains("tap"));
    }
}
