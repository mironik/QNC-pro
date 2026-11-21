//! The one rule of what every project's settings have. A template may leave fields out; before
//! the settings are written, this block completes them exactly as the old QNC did
//! (`complete_project_settings`): the product folders, the artifact modes, the audio editing
//! and loudness defaults, the export defaults and the AI block. A value a template gives is
//! never changed. Nothing here reads or writes a file or a database.

mod selection;

use serde_json::{json, Map, Value};

pub use selection::{apply_application_selection, project_only, validate_selection, CATALOG_URI};

/// The product folders of a project, relative to its folder.
pub const PRODUCT_FOLDERS: [(&str, &str); 9] = [
    ("root", "products"),
    ("thumbnails", "products/thumbnails"),
    ("filmstrip", "products/filmstrip"),
    ("virtual_shorts", "products/virtual_shorts"),
    ("virtual_segments", "products/virtual_segments"),
    ("b_roll_virtual_clips", "products/b_roll_virtual_clips"),
    ("prerender", "products/prerender"),
    ("voice", "products/voice"),
    ("ai_shots", "products/ai_shots"),
];
pub const CROSSFADE_FRAMES: u64 = 4;
pub const READING_WORDS_PER_SECOND: f64 = 2.5;
pub const TAIL_SECONDS: u64 = 2;
pub const LEAD_IN_SECONDS: u64 = 2;
pub const START_TIMECODE: &str = "10:00:00:00";
pub const VIDEO_BITRATE_MBPS: u64 = 35;
pub const SPEECH_LANGUAGE: &str = "hr";

fn loudness() -> Value {
    json!({ "a1_lufs": -23, "a2_lufs": -23, "true_peak_dbtp": -1, "tolerance_lu": 0.5 })
}

fn object<'a>(root: &'a mut Map<String, Value>, field: &str) -> Result<&'a mut Map<String, Value>, String> {
    root.entry(field.to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| qnc_text::fill("defaults.not_an_object", &[("field", field)]))
}

/// Completes `settings` in place; a field the template gave stays as it is.
pub fn complete(settings: &mut Value) -> Result<(), String> {
    let root = settings.as_object_mut().ok_or_else(|| qnc_text::fill("defaults.not_an_object", &[("field", "settings")]))?;
    let products = object(root, "products")?;
    for (field, folder) in PRODUCT_FOLDERS {
        products.entry(field.to_string()).or_insert_with(|| json!(folder));
    }
    let artifacts = object(root, "artifacts")?;
    for field in ["filmstrip", "wave"] {
        artifacts.entry(field.to_string()).or_insert_with(|| json!("auto"));
    }
    let audio = object(root, "audio")?;
    audio.entry("crossfade_frames".to_string()).or_insert_with(|| json!(CROSSFADE_FRAMES));
    audio.entry("reading_words_per_second".to_string()).or_insert_with(|| json!(READING_WORDS_PER_SECOND));
    audio.entry("loudness".to_string()).or_insert_with(loudness);
    let export = object(root, "export")?;
    export.entry("tail_seconds".to_string()).or_insert_with(|| json!(TAIL_SECONDS));
    export.entry("lead_in_seconds".to_string()).or_insert_with(|| json!(LEAD_IN_SECONDS));
    export.entry("start_timecode".to_string()).or_insert_with(|| json!(START_TIMECODE));
    export.entry("video_bitrate_mbps".to_string()).or_insert_with(|| json!(VIDEO_BITRATE_MBPS));
    let ai = object(root, "ai")?;
    for old in ["enabled", "coverage_suggestions", "transcription"] {
        ai.remove(old);
    }
    ai.entry("template".to_string()).or_insert_with(|| json!("none"));
    ai.entry("features".to_string()).or_insert_with(|| json!([]));
    ai.entry("language".to_string()).or_insert_with(|| json!(SPEECH_LANGUAGE));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_template_leaves_out_is_completed_and_what_it_gives_stays() {
        let mut settings = json!({ "audio": { "sample_rate": 48000, "crossfade_frames": 6 }, "export": { "directory": "exports/projekti" } });
        complete(&mut settings).unwrap();
        assert_eq!(settings["products"]["thumbnails"], "products/thumbnails");
        assert_eq!(settings["artifacts"]["wave"], "auto");
        assert_eq!(settings["audio"]["crossfade_frames"], 6);
        assert_eq!(settings["audio"]["reading_words_per_second"], 2.5);
        assert_eq!(settings["audio"]["loudness"]["a1_lufs"], -23);
        assert_eq!(settings["export"]["start_timecode"], "10:00:00:00");
        assert_eq!(settings["export"]["directory"], "exports/projekti");
        assert_eq!(settings["ai"]["language"], "hr");
        assert_eq!(settings["ai"]["features"], json!([]));
        let mut again = settings.clone();
        complete(&mut again).unwrap();
        assert_eq!(again, settings, "completing twice changes nothing");
        assert!(complete(&mut json!({ "products": 3 })).is_err());
    }
}
