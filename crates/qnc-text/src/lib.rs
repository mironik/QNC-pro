//! Every label has a code. The dictionaries live in `contracts/text/<language>.json` and are
//! built into the program; the language is chosen once per process (the computer's setting,
//! read by whoever starts the program). A code no dictionary knows is shown as the code itself,
//! so a missing text is visible, never invented.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

pub const BASE_LANGUAGE: &str = "en";

/// (language code, native name, dictionary text)
pub const DICTIONARIES: &[(&str, &str, &str)] = &[
    ("en", "English", include_str!("../../../contracts/text/en.json")),
    ("hr", "Hrvatski", include_str!("../../../contracts/text/hr.json")),
];

pub struct Texts {
    language: String,
    texts: BTreeMap<String, String>,
}

impl Texts {
    pub fn of(language: &str) -> Result<Texts, String> {
        let (_, _, json) = DICTIONARIES
            .iter()
            .find(|(code, _, _)| *code == language)
            .ok_or_else(|| format!("no dictionary for the language {language}"))?;
        let texts: BTreeMap<String, String> =
            serde_json::from_str(json).map_err(|e| format!("dictionary {language}: {e}"))?;
        Ok(Texts { language: language.to_string(), texts })
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
pub fn languages() -> Vec<(&'static str, &'static str)> {
    DICTIONARIES.iter().map(|(code, name, _)| (*code, *name)).collect()
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
        for (language, _, _) in DICTIONARIES {
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
