//! The start every Project window shares (the Projects form, the Settings form): the QNC root,
//! the diagnostics, one copy per session, the language of this computer, the Project actions
//! checked against the key catalog, the look of the form, the layout with the look put in and
//! the session over the component. The forms only draw over what this gives.

use std::path::PathBuf;

use qnc_project_layout::ProjectLayout;
use qnc_project_session::{ProjectSession, ACTION_IDS};

pub struct Started {
    pub root: PathBuf,
    pub look: qnc_form_look_model::FormLook,
    pub layout: ProjectLayout,
    pub session: ProjectSession,
    _one: qnc_one_copy::OneCopy,
}

/// Starts the window `program` as one copy (`None`: another copy runs), with the look of `form`
/// that must have `keys`.
pub fn start(program: &'static str, form: &str, keys: &[&str]) -> Result<Option<Started>, String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), program);
    qnc_diagnostics::log_panics();
    let Some(one) = qnc_one_copy::OneCopy::take(program)? else { return Ok(None) };
    qnc_settings::choose_language(&root)?;
    let missing = ProjectLayout::actions_missing(ACTION_IDS);
    if !missing.is_empty() {
        return Err(format!("the key catalog lacks the Project actions {missing:?}"));
    }
    let look = qnc_form_look_model::FormLook::load(&root, form, keys)?;
    let layout = ProjectLayout::load(&look.pointed())?;
    let ready = layout.left_project_list.ready_text.clone();
    let component = qnc_project_application::ProjectComponent::new(&root);
    let session = ProjectSession::new(component, ready, ACTION_IDS.iter().map(|id| id.to_string()).collect());
    qnc_diagnostics::log(format!("{program} started"));
    Ok(Some(Started { root, look, layout, session, _one: one }))
}

/// The layout with the look's measures put in, when they moved since `stamp`.
pub fn relayout(look: &qnc_form_look_model::FormLook, stamp: &mut Vec<f32>) -> Option<ProjectLayout> {
    let now = look.stamp();
    if now == *stamp {
        return None;
    }
    *stamp = now;
    match ProjectLayout::load(&look.pointed()) {
        Ok(layout) => Some(layout),
        Err(error) => {
            qnc_diagnostics::log(format!("look not applied: {error}"));
            None
        }
    }
}
