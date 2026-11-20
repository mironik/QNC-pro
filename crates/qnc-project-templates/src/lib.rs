//! A project template is data, not code: one JSON file per template under
//! `contracts/project/templates` with its id, name, description, the tray modes it serves and
//! the settings a new project starts from. This block reads them all, refuses a file with an
//! error by name (the rest are taken), finds one by id and names the one a tray mode uses.
//! It decides nothing about a project; the caller chooses.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub template_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The tray modes this template serves (`breaking_news`, `story`, `net`).
    #[serde(default)]
    pub modes: Vec<String>,
    pub settings: Value,
}

/// The templates folder under the QNC root.
pub fn folder(root: &Path) -> PathBuf {
    root.join("contracts").join("project").join("templates")
}

fn parse(text: &str, file: &str) -> Result<Template, String> {
    let template: Template = serde_json::from_str(text).map_err(|e| qnc_text::fill("template.unreadable", &[("file", file), ("error", &e.to_string())]))?;
    if template.template_id.trim().is_empty() || template.name.trim().is_empty() || !template.settings.is_object() {
        return Err(qnc_text::fill("template.unreadable", &[("file", file), ("error", "template_id, name and settings are needed")]));
    }
    Ok(template)
}

/// Every template in `folder`, by id; a file with an error is named in the second list.
pub fn read_templates(folder: &Path) -> (Vec<Template>, Vec<String>) {
    let mut templates = Vec::new();
    let mut refused = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        refused.push(qnc_text::fill("template.unreadable", &[("file", &folder.display().to_string()), ("error", "the folder cannot be read")]));
        return (templates, refused);
    };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for file in files {
        let name = file.display().to_string();
        match std::fs::read_to_string(&file).map_err(|e| qnc_text::fill("template.unreadable", &[("file", &name), ("error", &e.to_string())])).and_then(|text| parse(&text, &name)) {
            Ok(template) => match templates.iter().any(|t: &Template| t.template_id == template.template_id) {
                true => refused.push(qnc_text::fill("template.unreadable", &[("file", &name), ("error", "a template with this id was already read")])),
                false => templates.push(template),
            },
            Err(why) => refused.push(why),
        }
    }
    templates.sort_by(|a, b| a.template_id.cmp(&b.template_id));
    (templates, refused)
}

/// The template with this id.
pub fn by_id<'a>(templates: &'a [Template], id: &str) -> Result<&'a Template, String> {
    templates.iter().find(|t| t.template_id == id).ok_or_else(|| qnc_text::fill("template.unknown", &[("id", id)]))
}

/// The template a tray mode makes its projects from.
pub fn for_mode<'a>(templates: &'a [Template], mode: &str) -> Result<&'a Template, String> {
    templates.iter().find(|t| t.modes.iter().any(|m| m == mode)).ok_or_else(|| qnc_text::fill("template.none_for_mode", &[("mode", mode)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_templates_are_read_each_mode_has_one_and_a_broken_file_is_named() {
        let shipped = folder(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
        let (templates, refused) = read_templates(&shipped);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(templates.iter().map(|t| t.template_id.as_str()).collect::<Vec<_>>(), ["breaking_news", "field_sony", "house_xdcam", "news_package"]);
        assert_eq!(for_mode(&templates, "breaking_news").unwrap().template_id, "breaking_news");
        assert_eq!(for_mode(&templates, "net").unwrap().template_id, "breaking_news");
        assert_eq!(for_mode(&templates, "story").unwrap().template_id, "news_package");
        assert!(for_mode(&templates, "nothing").is_err());
        assert_eq!(by_id(&templates, "house_xdcam").unwrap().settings["video"]["fps"], 25);
        assert!(by_id(&templates, "x").is_err());
        let dir = std::env::temp_dir().join(format!("qnc-project-templates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), r#"{ "template_id": "a", "name": "A", "settings": {} }"#).unwrap();
        std::fs::write(dir.join("b.json"), r#"{ "template_id": "b", "settings": {} }"#).unwrap();
        std::fs::write(dir.join("c.json"), r#"{ "template_id": "a", "name": "A again", "settings": {} }"#).unwrap();
        let (templates, refused) = read_templates(&dir);
        assert_eq!(templates.len(), 1);
        assert_eq!(refused.len(), 2, "{refused:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
