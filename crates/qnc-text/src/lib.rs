//! The words of the QNC user interface (copied from `qnc-ui-text` of QNC-shared; user decision
//! 2026-10-14: everything in English, every text with a code, so the language can be chosen).
//! The layout contracts write a text as `@code`; this block replaces it with the text of the
//! chosen language from the dictionaries in `contracts/text`, built into the program. A
//! dictionary may be in parts; a code in two parts of one language is an error. The language
//! is chosen once per process (the computer's setting, read by whoever starts the program). A
//! code no dictionary knows is shown as the code itself, so a missing text is visible, never
//! invented.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

pub const BASE_LANGUAGE: &str = "en";

/// The dictionaries QNC is built with: language code and its parts.
pub const DICTIONARIES: &[(&str, &[&str])] = &[
    ("en", &[include_str!("../../../contracts/text/en.json"), include_str!("../../../contracts/text/en-pro.json")]),
    (
        "hr",
        &[
            include_str!("../../../contracts/text/hr/1.json"),
            include_str!("../../../contracts/text/hr/2.json"),
            include_str!("../../../contracts/text/hr/3.json"),
            include_str!("../../../contracts/text/hr/4.json"),
            include_str!("../../../contracts/text/hr-pro.json"),
        ],
    ),
];

#[derive(serde::Deserialize)]
struct Dictionary {
    language: String,
    #[serde(default)]
    name: String,
    texts: BTreeMap<String, String>,
}

pub struct Texts {
    language: String,
    texts: BTreeMap<String, String>,
}

impl Texts {
    /// The dictionary of `language` from all its parts.
    pub fn of(language: &str) -> Result<Texts, String> {
        let (_, parts) = DICTIONARIES.iter().find(|(code, _)| *code == language).ok_or_else(|| format!("no dictionary for the language {language}"))?;
        let mut texts = BTreeMap::new();
        for json in parts.iter() {
            let file: Dictionary = serde_json::from_str(json).map_err(|e| format!("dictionary {language}: {e}"))?;
            if file.language != language {
                return Err(format!("dictionary {language} is marked as {}", file.language));
            }
            for (code, text) in file.texts {
                if texts.insert(code.clone(), text).is_some() {
                    return Err(format!("dictionary {language} has {code} twice"));
                }
            }
        }
        Ok(Texts { language: language.to_string(), texts })
    }

    /// As [`Texts::of`] (the name the copied code uses).
    pub fn for_language(language: &str) -> Result<Texts, String> {
        Texts::of(language)
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn get(&self, code: &str) -> Option<&str> {
        self.texts.get(code).map(String::as_str)
    }

    pub fn codes(&self) -> impl Iterator<Item = &str> {
        self.texts.keys().map(String::as_str)
    }
}

static CHOSEN: OnceLock<Texts> = OnceLock::new();

/// Chooses the language of this process. Only the first choice counts; later ones are ignored,
/// because labels already shown must not change their language mid-run.
pub fn choose(language: &str) -> Result<(), String> {
    let texts = Texts::of(language)?;
    let _ = CHOSEN.set(texts);
    Ok(())
}

fn chosen() -> &'static Texts {
    CHOSEN.get_or_init(|| Texts::of(BASE_LANGUAGE).expect("the base dictionary is built in"))
}

/// The text of a code, or the code itself when no dictionary has it.
pub fn text(code: &'static str) -> &'static str {
    chosen().get(code).unwrap_or(code)
}

/// The text of a code with `{name}` places filled.
pub fn fill(code: &'static str, values: &[(&str, &str)]) -> String {
    let mut out = text(code).to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Languages QNC speaks: (code, native name).
pub fn languages() -> Vec<(&'static str, String)> {
    DICTIONARIES.iter().filter_map(|(code, parts)| serde_json::from_str::<Dictionary>(parts.first()?).ok().map(|d| (*code, if d.name.is_empty() { code.to_string() } else { d.name }))).collect()
}

/// A contract whose string values are `@code` gets the texts of those codes.
pub fn resolve_contract(json: &str) -> Result<Value, String> {
    let mut value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    resolve(&mut value);
    Ok(value)
}

fn resolve(value: &mut Value) {
    match value {
        Value::String(s) => {
            if let Some(code) = s.strip_prefix('@') {
                if let Some(t) = chosen().get(code) {
                    *s = t.to_string();
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(resolve),
        Value::Object(map) => map.values_mut().for_each(resolve),
        _ => {}
    }
}

/// The `@code`s a contract uses.
pub fn codes_in(value: &Value) -> Vec<String> {
    let mut codes = Vec::new();
    collect(value, &mut codes);
    codes.sort();
    codes.dedup();
    codes
}

fn collect(value: &Value, into: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            if let Some(code) = s.strip_prefix('@') {
                into.push(code.to_string());
            }
        }
        Value::Array(items) => items.iter().for_each(|v| collect(v, into)),
        Value::Object(map) => map.values().for_each(|v| collect(v, into)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dictionary_has_exactly_the_codes_of_the_base_and_no_empty_text() {
        let base = Texts::of(BASE_LANGUAGE).unwrap();
        for (language, _) in DICTIONARIES {
            let texts = Texts::of(language).unwrap();
            let codes: Vec<&str> = texts.codes().collect();
            assert_eq!(codes, base.codes().collect::<Vec<_>>(), "codes of {language}");
            for code in codes {
                assert!(!texts.get(code).unwrap().trim().is_empty(), "{language} {code} is empty");
            }
        }
    }

    #[test]
    fn a_missing_code_is_shown_as_the_code_and_places_are_filled() {
        assert_eq!(text("no.such.code"), "no.such.code");
        assert_eq!(fill("db.no_module", &[("module", "x")]), "No table module is called x.");
        let value = resolve_contract(r#"{"a":"@common.ok","b":["@none"],"c":1}"#).unwrap();
        assert_eq!(value["a"], "OK");
        assert_eq!(value["b"][0], "@none");
        assert_eq!(codes_in(&serde_json::json!({"x":"@a.b","y":{"z":"@c"}})), vec!["a.b", "c"]);
    }
}
