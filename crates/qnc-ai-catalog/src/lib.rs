//! The AI catalog of this computer (the catalog `ai` of the QNC root): which program, model and
//! backend transcribe speech here. Computer configuration, never a project setting: two laptops
//! run the same project with their own engines. One engine per purpose is selected; one that
//! cannot run (its model missing, an unknown backend) is an error, nothing falls back to another.
//! Models named `share:...` are in the shared AI folder of the computer (the setting `ai-share`,
//! or `QNC_AI_SHARE`), which QNC and QNC-AI both read; nothing is guessed when it is not set.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The catalog's name (`catalogs/ai.json` of the root, read through the settings block).
pub const CATALOG: &str = "ai";
/// The computer setting naming the shared AI folder: `{"folder": "..."}`.
pub const SHARE_SETTING: &str = "ai-share";
/// The backends an engine may name.
pub const BACKENDS: [&str; 3] = ["cpu", "vulkan", "directml"];
/// A model named under this prefix is in the shared AI folder.
pub const SHARE_PREFIX: &str = "share:";

/// Which engine transcribes: each its own, so both can run at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// The best text, as fast as the computer allows.
    Best,
    /// Fast on the processor: the ingest's pass over every clip.
    Fast,
}

/// The engine that transcribes speech on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcriber {
    pub id: String,
    /// The program's name beside the application.
    pub program: String,
    /// One of [`BACKENDS`].
    pub backend: String,
    pub model: PathBuf,
    pub vad_model: Option<PathBuf>,
    pub threads: u32,
    /// How many clips are heard at the same time, each in a program of its own (at least 1).
    pub parallel: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: String,
    transcribe: Job,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    selected: String,
    #[serde(default)]
    fast: Option<String>,
    #[serde(default)]
    fast_with: Vec<String>,
    engines: Vec<Engine>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Engine {
    id: String,
    program: String,
    backend: String,
    model: String,
    #[serde(default)]
    vad_model: Option<String>,
    threads: u32,
    #[serde(default)]
    parallel: Option<u32>,
}

/// The shared AI folder of this computer: `QNC_AI_SHARE`, else the setting; not set is an error.
pub fn share_dir(root: &Path) -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("QNC_AI_SHARE").filter(|dir| !dir.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let setting = qnc_settings::setting(root, SHARE_SETTING)?.ok_or_else(|| qnc_text::text("ai.share_not_set").to_string())?;
    let folder = setting.get("folder").and_then(|f| f.as_str()).ok_or_else(|| qnc_text::text("ai.share_not_set").to_string())?;
    let folder = PathBuf::from(folder);
    Ok(if folder.is_absolute() { folder } else { root.join(folder) })
}

/// Where a model the catalog names is: under the shared AI folder for `share:...`, else beside
/// the catalog.
pub fn model_path(root: &Path, named: &str) -> Result<PathBuf, String> {
    match named.strip_prefix(SHARE_PREFIX) {
        Some(rest) => Ok(share_dir(root)?.join(rest)),
        None => Ok(qnc_settings::catalog_path(root, CATALOG).parent().ok_or("the catalog has no folder")?.join(named)),
    }
}

/// The name a model is asked by in a plug-in request: its file or folder name, the same on
/// every computer that has it.
pub fn model_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn file(root: &Path) -> Result<File, String> {
    let value = qnc_settings::catalog(root, CATALOG)?;
    let file: File = serde_json::from_value(value).map_err(|e| qnc_text::fill("ai.catalog_invalid", &[("error", &e.to_string())]))?;
    if file.version != "1" {
        return Err(qnc_text::fill("ai.catalog_invalid", &[("error", &format!("version {}", file.version))]));
    }
    Ok(file)
}

fn engine_in(root: &Path, file: &File, id: &str) -> Result<Transcriber, String> {
    let engine = file.transcribe.engines.iter().find(|engine| engine.id == id).ok_or_else(|| qnc_text::fill("ai.no_engine", &[("engine", id)]))?;
    if !BACKENDS.contains(&engine.backend.as_str()) {
        return Err(qnc_text::fill("ai.catalog_invalid", &[("error", &format!("{}: unknown backend {}", engine.id, engine.backend))]));
    }
    if engine.threads == 0 || engine.parallel == Some(0) {
        return Err(qnc_text::fill("ai.catalog_invalid", &[("error", &format!("{}: threads and parallel are at least 1", engine.id))]));
    }
    let present = |named: &str| -> Result<PathBuf, String> {
        let path = model_path(root, named)?;
        if path.exists() {
            Ok(path)
        } else {
            Err(qnc_text::fill("ai.model_missing", &[("engine", &engine.id), ("model", &path.display().to_string())]))
        }
    };
    Ok(Transcriber {
        id: engine.id.clone(),
        program: engine.program.clone(),
        backend: engine.backend.clone(),
        model: present(&engine.model)?,
        vad_model: engine.vad_model.as_deref().map(present).transpose()?,
        threads: engine.threads,
        parallel: engine.parallel.unwrap_or(1),
    })
}

/// The engine `id` of this computer's catalog, whatever is selected (the program finds the
/// engine it is asked by name).
pub fn engine(root: &Path, id: &str) -> Result<Transcriber, String> {
    engine_in(root, &file(root)?, id)
}

/// The engine this computer selects for `quality`; a catalog without a fast one gives none.
pub fn transcriber_for(root: &Path, quality: Quality) -> Result<Transcriber, String> {
    let file = file(root)?;
    let id = match quality {
        Quality::Best => file.transcribe.selected.clone(),
        Quality::Fast => file.transcribe.fast.clone().ok_or_else(|| qnc_text::text("ai.no_fast_engine").to_string())?,
    };
    engine_in(root, &file, &id)
}

/// Every engine that hears the fast pieces: the fast one first, then those it is given with.
pub fn fast_transcribers(root: &Path) -> Result<Vec<Transcriber>, String> {
    let file = file(root)?;
    let mut engines = vec![transcriber_for(root, Quality::Fast)?];
    for id in &file.transcribe.fast_with {
        engines.push(engine_in(root, &file, id)?);
    }
    Ok(engines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("qnc-ai-catalog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts").join("modules")).unwrap();
        std::fs::create_dir_all(qnc_settings::catalog_path(&root, CATALOG).parent().unwrap()).unwrap();
        root
    }

    fn catalog(root: &Path, selected: &str, fast: Option<&str>, backend: &str) {
        let fast = fast.map_or(String::new(), |fast| format!(r#""fast":"{fast}","#));
        std::fs::write(
            qnc_settings::catalog_path(root, CATALOG),
            format!(
                r#"{{"version":"1","transcribe":{{"selected":"{selected}",{fast}"engines":[
                  {{"id":"w.gpu","program":"qnc-transcribe-whisper","backend":"{backend}","model":"model.bin","vad_model":"vad.bin","threads":4}},
                  {{"id":"p.cpu","program":"qnc-transcribe-parakeet","backend":"cpu","model":"share:models/parakeet","threads":2,"parallel":3}}]}}}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn the_selected_engine_is_given_with_its_models_and_the_fast_one_is_its_own() {
        let root = root("ok");
        let folder = qnc_settings::catalog_path(&root, CATALOG).parent().unwrap().to_path_buf();
        std::fs::write(folder.join("model.bin"), b"m").unwrap();
        std::fs::write(folder.join("vad.bin"), b"v").unwrap();
        let share = root.join("share");
        std::fs::create_dir_all(share.join("models").join("parakeet")).unwrap();
        qnc_settings::set_setting(&root, SHARE_SETTING, &serde_json::json!({ "folder": share })).unwrap();
        catalog(&root, "w.gpu", Some("p.cpu"), "vulkan");
        let best = transcriber_for(&root, Quality::Best).unwrap();
        assert_eq!((best.backend.as_str(), best.model.clone(), best.vad_model.clone()), ("vulkan", folder.join("model.bin"), Some(folder.join("vad.bin"))));
        let fast = transcriber_for(&root, Quality::Fast).unwrap();
        assert_eq!((fast.program.as_str(), fast.model.clone(), fast.parallel), ("qnc-transcribe-parakeet", share.join("models").join("parakeet"), 3));
        assert_eq!(fast_transcribers(&root).unwrap().len(), 1);
        assert_eq!(model_name(&fast.model), "parakeet");
        assert_eq!(engine(&root, "p.cpu").unwrap().id, "p.cpu");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_model_an_unknown_choice_or_no_fast_engine_is_an_error_not_another_engine() {
        let root = root("missing");
        catalog(&root, "w.gpu", None, "vulkan");
        assert!(transcriber_for(&root, Quality::Best).unwrap_err().contains("model.bin"));
        let folder = qnc_settings::catalog_path(&root, CATALOG).parent().unwrap().to_path_buf();
        std::fs::write(folder.join("model.bin"), b"m").unwrap();
        std::fs::write(folder.join("vad.bin"), b"v").unwrap();
        assert!(transcriber_for(&root, Quality::Fast).is_err(), "no fast engine");
        assert!(engine(&root, "w.none").is_err());
        catalog(&root, "w.gpu", None, "cuda");
        assert!(transcriber_for(&root, Quality::Best).is_err(), "unknown backend");
        assert!(share_dir(&root).is_err(), "the shared folder is not guessed");
        let _ = std::fs::remove_dir_all(&root);
    }
}
