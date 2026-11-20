//! Every input of QNC ends as an action id: a key, a mouse gesture, a button, a touchpad all
//! send the same id. The catalog (`contracts/keyboard-shortcuts.json`) is built in; a chord is
//! written as `Ctrl+Shift+I`. Keys are named as on a keyboard (`A`..`Z`, `0`..`9`, `Enter`,
//! `Escape`, `Space`, `Left`, `F1`, `/`, `[`); a drawing kit maps its window's keys to these
//! names, this block never sees a window.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use serde::Deserialize;

pub const CATALOG: &str = include_str!("../../../contracts/keyboard-shortcuts.json");

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Chord {
    pub fn plain(key: &str) -> Chord {
        Chord { key: key.to_string(), ctrl: false, shift: false, alt: false }
    }

    /// `Ctrl+Shift+I`: modifiers in any order, the key last.
    pub fn parse(text: &str) -> Result<Chord, String> {
        let mut chord = Chord::plain("");
        let parts: Vec<&str> = text.split('+').collect();
        let (key, modifiers) = parts.split_last().ok_or_else(|| bad(text))?;
        for modifier in modifiers {
            match *modifier {
                "Ctrl" => chord.ctrl = true,
                "Shift" => chord.shift = true,
                "Alt" => chord.alt = true,
                _ => return Err(bad(text)),
            }
        }
        if key.is_empty() {
            return Err(bad(text));
        }
        chord.key = key.to_string();
        Ok(chord)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [(self.ctrl, "Ctrl+"), (self.shift, "Shift+"), (self.alt, "Alt+")] {
            if on {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

fn bad(chord: &str) -> String {
    qnc_text::fill("actions.bad_chord", &[("chord", chord)])
}

#[derive(Debug, Clone, Deserialize)]
pub struct Action {
    /// A label code of the dictionary.
    pub label: String,
    pub group: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Raw {
    actions: BTreeMap<String, Action>,
    groups: BTreeMap<String, String>,
    rules: Vec<String>,
    presets: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
}

#[derive(Debug)]
pub struct Catalog {
    raw: Raw,
    /// preset -> scope -> chord -> action id
    keys: BTreeMap<String, BTreeMap<String, BTreeMap<Chord, String>>>,
}

impl Catalog {
    pub fn parse(json: &str) -> Result<Catalog, String> {
        let raw: Raw = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut keys = BTreeMap::new();
        for (preset, scopes) in &raw.presets {
            let mut by_scope = BTreeMap::new();
            for (scope, bindings) in scopes {
                let mut by_chord: BTreeMap<Chord, String> = BTreeMap::new();
                for (text, action) in bindings {
                    let chord = Chord::parse(text)?;
                    if !raw.actions.contains_key(action) {
                        return Err(qnc_text::fill("actions.unknown_action", &[("chord", text), ("action", action)]));
                    }
                    if by_chord.insert(chord, action.clone()).is_some() {
                        return Err(qnc_text::fill("actions.chord_twice", &[("chord", text), ("scope", scope)]));
                    }
                }
                by_scope.insert(scope.clone(), by_chord);
            }
            keys.insert(preset.clone(), by_scope);
        }
        Ok(Catalog { raw, keys })
    }

    pub fn has(&self, action: &str) -> bool {
        self.raw.actions.contains_key(action)
    }

    pub fn action(&self, id: &str) -> Option<&Action> {
        self.raw.actions.get(id)
    }

    pub fn presets(&self) -> impl Iterator<Item = &str> {
        self.raw.presets.keys().map(String::as_str)
    }

    /// The scopes of a preset.
    pub fn scopes(&self, preset: &str) -> Vec<&str> {
        self.keys.get(preset).map(|scopes| scopes.keys().map(String::as_str).collect()).unwrap_or_default()
    }

    /// The action a chord gives in one scope of a preset.
    pub fn action_for(&self, preset: &str, scope: &str, chord: &Chord) -> Option<&str> {
        self.keys.get(preset)?.get(scope)?.get(chord).map(String::as_str)
    }

    /// The chords of one scope: (chord, action id).
    pub fn bindings(&self, preset: &str, scope: &str) -> Vec<(&Chord, &str)> {
        self.keys
            .get(preset)
            .and_then(|scopes| scopes.get(scope))
            .map(|by_chord| by_chord.iter().map(|(c, a)| (c, a.as_str())).collect())
            .unwrap_or_default()
    }

    /// Group id -> label code.
    pub fn groups(&self) -> &BTreeMap<String, String> {
        &self.raw.groups
    }

    /// The input rules, as label codes, for the help window.
    pub fn rules(&self) -> &[String] {
        &self.raw.rules
    }
}

static BUILT_IN: OnceLock<Catalog> = OnceLock::new();

/// The built-in catalog.
pub fn catalog() -> &'static Catalog {
    BUILT_IN.get_or_init(|| Catalog::parse(CATALOG).expect("the built-in catalog is valid; its test says so"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_catalog_parses_and_every_label_has_a_text() {
        let catalog = Catalog::parse(CATALOG).unwrap();
        let texts = qnc_text::Texts::of(qnc_text::BASE_LANGUAGE).unwrap();
        for (id, action) in &catalog.raw.actions {
            let code = action.label.strip_prefix('@').unwrap_or_else(|| panic!("{id} label is not a code"));
            assert!(texts.get(code).is_some(), "{id}: no text for {code}");
            assert!(catalog.raw.groups.contains_key(&action.group), "{id}: unknown group");
        }
        for code in catalog.raw.groups.values().chain(catalog.raw.rules.iter()) {
            assert!(texts.get(code.trim_start_matches('@')).is_some(), "no text for {code}");
        }
    }

    #[test]
    fn a_chord_is_read_and_written_the_same_and_gives_its_action() {
        let chord = Chord::parse("Ctrl+Shift+I").unwrap();
        assert_eq!(chord.to_string(), "Ctrl+Shift+I");
        assert_eq!(Chord::parse("Shift+Ctrl+I").unwrap(), chord);
        assert!(Chord::parse("Meta+I").is_err());
        assert!(Chord::parse("Ctrl+").is_err());
        let catalog = catalog();
        assert_eq!(catalog.action_for("qnc", "source_timeline", &Chord::parse("Ctrl+I").unwrap()), Some("select_mark_in"));
        assert_eq!(catalog.action_for("qnc", "common", &Chord::plain("Enter")), Some("activate_focused_item"));
        assert_eq!(catalog.action_for("qnc", "common", &Chord::plain("I")), None);
    }

    #[test]
    fn a_binding_to_an_unknown_action_or_a_chord_bound_twice_is_refused() {
        let unknown = r#"{"actions":{},"groups":{},"rules":[],"presets":{"p":{"s":{"I":"nothing"}}}}"#;
        assert!(Catalog::parse(unknown).unwrap_err().contains("nothing"));
        let twice = r#"{"actions":{"a":{"label":"@x","group":"g"}},"groups":{},"rules":[],"presets":{"p":{"s":{"Ctrl+Shift+I":"a","Shift+Ctrl+I":"a"}}}}"#;
        assert!(Catalog::parse(twice).unwrap_err().contains("twice"));
    }
}
