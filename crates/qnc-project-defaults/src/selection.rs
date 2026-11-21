//! The applications a project works with (copied from the old QNC's `apply_application_selection`
//! and the selection snapshot's rule): the selection is a snapshot (`schema_version` 1, the
//! catalog URI, when it was observed, the applications with their tab, label and priority
//! group, unique and in ascending groups, group `a` first), and it is written into the settings
//! as `workspace.application_selection`, `workspace.tabs` and `workspace.tab_labels`.

use serde_json::{json, Value};

/// The catalog of applications a selection names.
pub const CATALOG_URI: &str = "qnc://local/catalog/applications";

/// A selection of the one application `qnc.project` (what a project made without the form has).
pub fn project_only() -> Value {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1);
    json!({
        "schema_version": 1,
        "catalog_uri": CATALOG_URI,
        "observed_at_unix_ms": now,
        "applications": [{ "application_id": "qnc.project", "tab_id": "project", "label": "project", "priority_group": "a" }]
    })
}

fn token(value: &str, dots: bool) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || (dots && c == '.'))
}

/// Refuses a selection that is not one: version and observation, unique applications and tabs,
/// strictly ascending one-letter groups, at least one application and one of group `a`.
pub fn validate_selection(selection: &Value) -> Result<(), String> {
    if selection["schema_version"].as_u64() != Some(1) || selection["observed_at_unix_ms"].as_u64().is_none_or(|t| t == 0) {
        return Err("Invalid application selection version/observation".into());
    }
    qnc_uri::parse(selection["catalog_uri"].as_str().unwrap_or(""))?;
    let apps = selection["applications"].as_array().ok_or("the selection has no applications")?;
    let mut ids = std::collections::HashSet::new();
    let mut tabs = std::collections::HashSet::new();
    let mut previous: Option<&str> = None;
    for app in apps {
        let (id, tab, label, group) = (app["application_id"].as_str().unwrap_or(""), app["tab_id"].as_str().unwrap_or(""), app["label"].as_str().unwrap_or(""), app["priority_group"].as_str().unwrap_or(""));
        if !token(id, true) || !token(tab, true) || label.trim().is_empty() || group.len() != 1 || !group.as_bytes()[0].is_ascii_lowercase() {
            return Err(format!("Invalid application identity: {id} {tab} {group}"));
        }
        if !ids.insert(id) || !tabs.insert(tab) || previous.is_some_and(|p| p >= group) {
            return Err("Selection must have unique apps/tabs and strictly ascending groups".into());
        }
        previous = Some(group);
    }
    if apps.is_empty() {
        return Err(qnc_text::text("app.choose_at_least_one_application").into());
    }
    if !apps.iter().any(|app| app["priority_group"] == "a") {
        return Err(qnc_text::text("app.choose_application_from_group").into());
    }
    Ok(())
}

/// Writes `selection` into `settings` as the old QNC did.
pub fn apply_application_selection(settings: &mut Value, selection: &Value) -> Result<(), String> {
    validate_selection(selection)?;
    let apps = selection["applications"].as_array().cloned().unwrap_or_default();
    let root = settings.as_object_mut().ok_or(qnc_text::text("project.store.settings_object"))?;
    let workspace = root.entry("workspace").or_insert_with(|| json!({})).as_object_mut().ok_or(qnc_text::text("project.store.workspace_object"))?;
    workspace.insert("application_selection".into(), selection.clone());
    workspace.insert("tabs".into(), json!(apps.iter().map(|app| app["tab_id"].clone()).collect::<Vec<_>>()));
    workspace.insert("tab_labels".into(), Value::Object(apps.iter().map(|app| (app["tab_id"].as_str().unwrap_or("").to_string(), app["label"].clone())).collect()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_goes_into_the_workspace_of_the_settings_and_a_bad_one_is_refused() {
        let mut settings = json!({});
        let selection = project_only();
        apply_application_selection(&mut settings, &selection).unwrap();
        assert_eq!(settings["workspace"]["tabs"], json!(["project"]));
        assert_eq!(settings["workspace"]["tab_labels"]["project"], "project");
        assert_eq!(settings["workspace"]["application_selection"], selection);
        let mut twice = selection.clone();
        twice["applications"].as_array_mut().unwrap().push(json!({ "application_id": "qnc.ingest", "tab_id": "ingest", "label": "Ingest", "priority_group": "a" }));
        assert!(validate_selection(&twice).is_err(), "two of one group");
        let mut none = selection.clone();
        none["applications"] = json!([]);
        assert!(validate_selection(&none).is_err());
        let mut no_a = selection;
        no_a["applications"][0]["priority_group"] = json!("b");
        assert!(validate_selection(&no_a).is_err());
    }
}
