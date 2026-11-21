//! The look of a form as data (copied from the old QNC's `qnc-form-look`, its data part): every
//! measure the form draws with (margins, gaps, sizes, column widths) is a named value in the
//! form's look file `contracts/ui/forms/<form>.look.json` under the QNC root. The form asks its
//! values by key; a key the file lacks is an error when the file is read, never a value made up
//! in code. This block reads, moves and saves the values; `qnc-form-look` draws the editor.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// One measure of a form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Value {
    pub key: String,
    /// What the editor calls it.
    pub label: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// The step of the arrows and the slider (1 for pixels).
    pub step: f32,
    /// For a form drawn from a layout contract: where the value goes in it (a JSON pointer, e.g.
    /// `/right_settings_panel/row_height`); the form puts it there ([`FormLook::pointed`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct File {
    form: String,
    values: Vec<Value>,
}

/// The look of one form, as saved and as being edited.
pub struct FormLook {
    form: String,
    file: PathBuf,
    values: RefCell<Vec<Value>>,
    saved: Vec<Value>,
}

/// The look file of `form` under `root`.
pub fn look_file(root: &Path, form: &str) -> PathBuf {
    root.join("contracts").join("ui").join("forms").join(format!("{form}.look.json"))
}

/// The setting of this computer that opens the editor on every form.
pub fn editor_setting(root: &Path) -> PathBuf {
    qnc_settings::data_dir(root).join("form-look-editor.json")
}

/// Whether this computer's setting opens the editor; a broken setting is said and keeps it shut.
pub fn editor_on(root: &Path) -> bool {
    let path = editor_setting(root);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(setting) => setting.get("open").and_then(serde_json::Value::as_bool).unwrap_or(false),
        Err(error) => {
            qnc_diagnostics::log(format!("form-look setting {} unreadable: {error}", path.display()));
            false
        }
    }
}

fn snap(value: &mut Value) {
    value.value = ((value.value / value.step).round() * value.step).clamp(value.min, value.max);
}

impl FormLook {
    /// Reads the look of `form`; every key in `keys` must be there.
    pub fn load(root: &Path, form: &str, keys: &[&str]) -> Result<Self, String> {
        let file = look_file(root, form);
        let text = std::fs::read_to_string(&file).map_err(|error| format!("look file {}: {error}", file.display()))?;
        let read: File = serde_json::from_str(&text).map_err(|error| format!("look file {}: {error}", file.display()))?;
        if read.form != form {
            return Err(format!("look file {} is of form {}, not {form}", file.display(), read.form));
        }
        for value in &read.values {
            if value.min > value.max || !(value.min..=value.max).contains(&value.value) || value.step <= 0.0 {
                return Err(format!("look file {}: {} is out of its range", file.display(), value.key));
            }
        }
        if let Some(missing) = keys.iter().find(|key| !read.values.iter().any(|value| value.key == **key)) {
            return Err(format!("look file {} lacks {missing}", file.display()));
        }
        Ok(Self { form: form.to_string(), file, saved: read.values.clone(), values: RefCell::new(read.values) })
    }

    pub fn form(&self) -> &str {
        &self.form
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The value of `key` (checked present when read).
    pub fn get(&self, key: &str) -> f32 {
        self.values.borrow().iter().find(|value| value.key == key).map_or(0.0, |value| value.value)
    }

    /// Whether the look has `key`.
    pub fn has(&self, key: &str) -> bool {
        self.values.borrow().iter().any(|value| value.key == key)
    }

    /// Every value as it is now, to see whether it moved since the form last took it.
    pub fn stamp(&self) -> Vec<f32> {
        self.values.borrow().iter().map(|value| value.value).collect()
    }

    /// Every value that points into the form's layout contract, with its pointer.
    pub fn pointed(&self) -> Vec<(String, f32)> {
        self.values.borrow().iter().filter_map(|value| value.pointer.clone().map(|pointer| (pointer, value.value))).collect()
    }

    /// Whether the values differ from what is saved.
    pub fn changed(&self) -> bool {
        *self.values.borrow() != self.saved
    }

    /// Moves `key` by `by` within its range; `snapped` rounds it to its step.
    pub fn change(&self, key: &str, by: f32, snapped: bool) {
        if let Some(value) = self.values.borrow_mut().iter_mut().find(|value| value.key == key) {
            value.value = (value.value + by).clamp(value.min, value.max);
            if snapped {
                snap(value);
            }
        }
    }

    pub fn step_of(&self, key: &str) -> f32 {
        self.values.borrow().iter().find(|value| value.key == key).map_or(1.0, |value| value.step)
    }

    /// The label and value of `key`, as the editor shows them.
    pub fn shown(&self, key: &str) -> String {
        self.values.borrow().iter().find(|value| value.key == key).map(|value| format!("{}  {}", value.label, value.value)).unwrap_or_default()
    }

    /// The values and what is saved, for the editor's table.
    pub fn values(&self) -> &RefCell<Vec<Value>> {
        &self.values
    }

    pub fn saved(&self) -> &[Value] {
        &self.saved
    }

    /// Returns to what is saved.
    pub fn back(&self) {
        *self.values.borrow_mut() = self.saved.clone();
    }

    /// Writes the look file.
    pub fn save(&mut self) -> Result<(), String> {
        let file = File { form: self.form.clone(), values: self.values.borrow().clone() };
        let text = serde_json::to_string_pretty(&file).map_err(|error| error.to_string())?;
        std::fs::write(&self.file, text + "\n").map_err(|error| format!("{}: {error}", self.file.display()))?;
        self.saved = self.values.borrow().clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_gives_its_values_and_lacks_none_of_the_keys_the_form_asks() {
        let root = std::env::temp_dir().join(format!("qnc-form-look-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts/ui/forms")).unwrap();
        let file = File { form: "probe".into(), values: vec![Value { key: "margin".into(), label: "Margin".into(), value: 24.0, min: 0.0, max: 80.0, step: 1.0, pointer: Some("/panel/pad".into()) }] };
        std::fs::write(look_file(&root, "probe"), serde_json::to_string(&file).unwrap()).unwrap();
        let mut look = FormLook::load(&root, "probe", &["margin"]).unwrap();
        assert_eq!(look.get("margin"), 24.0);
        look.change("margin", 3.4, true);
        assert_eq!(look.get("margin"), 27.0, "a move is snapped to the step");
        look.change("margin", 500.0, true);
        assert_eq!(look.get("margin"), 80.0, "never past the range");
        assert!(look.changed());
        assert_eq!(look.pointed(), vec![("/panel/pad".to_string(), 80.0)]);
        look.save().unwrap();
        assert!(!look.changed());
        assert_eq!(FormLook::load(&root, "probe", &["margin"]).unwrap().get("margin"), 80.0, "saved");
        assert!(FormLook::load(&root, "probe", &["margin", "gap"]).is_err(), "a key the file lacks is an error");
        assert!(FormLook::load(&root, "other", &[]).is_err());
        assert!(!editor_on(&root));
        let _ = std::fs::remove_dir_all(&root);
    }
}
