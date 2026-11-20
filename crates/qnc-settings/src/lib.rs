//! Where QNC lives on this computer. The root is `QNC_ROOT` when set (and it must be a root),
//! otherwise the folder around the running program that has `contracts/modules`. Computer
//! settings are JSON files under `data/`, catalogs under `catalogs/`; nobody else builds those
//! paths. A setting is written whole, through a fresh file and a rename, so a reader never sees
//! half of it.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const MARKER: &str = "contracts/modules";

/// Is `path` a QNC root: a folder with `contracts/modules`.
pub fn is_root(path: &Path) -> bool {
    path.join(MARKER).is_dir()
}

/// The QNC root of this process.
pub fn root() -> Result<PathBuf, String> {
    if let Some(given) = std::env::var_os("QNC_ROOT") {
        let given = PathBuf::from(given);
        return is_root(&given)
            .then_some(given.clone())
            .ok_or_else(|| qnc_text::fill("settings.bad_root", &[("path", &given.display().to_string())]));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.ancestors()
        .skip(1)
        .take(5)
        .find(|dir| is_root(dir))
        .map(Path::to_path_buf)
        .ok_or_else(|| qnc_text::text("settings.no_root").to_string())
}

/// The folder of computer data (settings, logs, computer databases).
pub fn data_dir(root: &Path) -> PathBuf {
    root.join("data")
}

/// The folder the diagnostics block logs into.
pub fn diagnostics_dir(root: &Path) -> PathBuf {
    data_dir(root).join("diagnostics")
}

/// A catalog file `catalogs/<name>.json`.
pub fn catalog_path(root: &Path, name: &str) -> PathBuf {
    root.join("catalogs").join(format!("{name}.json"))
}

fn setting_path(root: &Path, name: &str) -> PathBuf {
    data_dir(root).join(format!("{name}.json"))
}

/// A computer setting `data/<name>.json`; `None` when the computer has not set it.
pub fn setting(root: &Path, name: &str) -> Result<Option<Value>, String> {
    let path = setting_path(root, name);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| unreadable(name, &e.to_string()))?;
    serde_json::from_str(&text).map(Some).map_err(|e| unreadable(name, &e.to_string()))
}

/// Writes a computer setting whole.
pub fn set_setting(root: &Path, name: &str, value: &Value) -> Result<(), String> {
    let path = setting_path(root, name);
    let dir = data_dir(root);
    fs::create_dir_all(&dir).map_err(|e| unwritable(name, &e.to_string()))?;
    let fresh = dir.join(format!("{name}.json.{}.writing", std::process::id()));
    let text = serde_json::to_string_pretty(value).map_err(|e| unwritable(name, &e.to_string()))?;
    fs::write(&fresh, text).map_err(|e| unwritable(name, &e.to_string()))?;
    fs::rename(&fresh, &path).map_err(|e| {
        let _ = fs::remove_file(&fresh);
        unwritable(name, &e.to_string())
    })
}

/// A catalog's content.
pub fn catalog(root: &Path, name: &str) -> Result<Value, String> {
    let path = catalog_path(root, name);
    let text = fs::read_to_string(&path).map_err(|e| unreadable(name, &e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| unreadable(name, &e.to_string()))
}

fn unreadable(name: &str, error: &str) -> String {
    qnc_text::fill("settings.unreadable", &[("name", name), ("error", error)])
}

fn unwritable(name: &str, error: &str) -> String {
    qnc_text::fill("settings.unwritable", &[("name", name), ("error", error)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qnc-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(MARKER)).unwrap();
        dir
    }

    #[test]
    fn a_root_has_contracts_modules_and_a_setting_is_none_until_written() {
        let dir = scratch();
        assert!(is_root(&dir));
        assert!(!is_root(&dir.join("contracts")));
        assert_eq!(setting(&dir, "language").unwrap(), None);
        set_setting(&dir, "language", &serde_json::json!({"language": "hr"})).unwrap();
        assert_eq!(setting(&dir, "language").unwrap().unwrap()["language"], "hr");
        assert!(!fs::read_dir(data_dir(&dir)).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains("writing")));
        assert_eq!(catalog_path(&dir, "decoders"), dir.join("catalogs").join("decoders.json"));
        let _ = fs::remove_dir_all(&dir);
    }
}
