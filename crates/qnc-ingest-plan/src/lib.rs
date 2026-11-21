//! The order of the steps of an ingest. The steps only do their work; the project's template
//! says in which order (`artifacts.order`): stages, a stage after the one before it, the steps of
//! one stage together. A project without the order is an error, never a default substituted;
//! [`DEFAULT`] is the baseline the system templates are seeded with.

use serde_json::Value;

/// One step of an ingest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// The clips copied or linked by the project's settings, each probed where it lies.
    Import,
    /// The posters: copied from the card, made from the first key picture only when the card has none.
    Posters,
    /// The first AI look, from the posters.
    PosterLook,
    Waves,
    Filmstrips,
    /// The transcription of the speech channel.
    Transcripts,
    /// The analysis of the shots.
    Shots,
    /// What the shots show (AI).
    Content,
    /// The sentences of what the shots show (AI).
    Captions,
}

impl Step {
    pub const ALL: [Step; 9] = [Step::Import, Step::Posters, Step::PosterLook, Step::Waves, Step::Filmstrips, Step::Transcripts, Step::Shots, Step::Content, Step::Captions];

    /// The code of the step: in the settings, in the stopped-work table and on its bar.
    pub fn code(self) -> &'static str {
        match self {
            Step::Import => "import",
            Step::Posters => "posters",
            Step::PosterLook => "poster_look",
            Step::Waves => "waves",
            Step::Filmstrips => "filmstrips",
            Step::Transcripts => "transcripts",
            Step::Shots => "shots",
            Step::Content => "content",
            Step::Captions => "captions",
        }
    }

    pub fn of(code: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|step| step.code() == code)
    }

    /// The steps whose results this one works from: they must be in an earlier stage or the same one.
    pub fn needs(self) -> &'static [Step] {
        match self {
            Step::Posters | Step::Waves | Step::Filmstrips | Step::Transcripts | Step::Shots => &[Step::Import],
            Step::PosterLook => &[Step::Posters],
            Step::Content => &[Step::Shots],
            Step::Captions => &[Step::Content],
            Step::Import => &[],
        }
    }
}

/// The stages of an ingest, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub stages: Vec<Vec<Step>>,
}

/// The baseline of the system templates.
pub const DEFAULT: &str = r#"[["import", "posters"], ["poster_look", "waves", "filmstrips", "transcripts"], ["shots", "content", "captions"]]"#;

impl Plan {
    /// The plan in `artifacts` of the project's settings (its `order`).
    pub fn of_settings(artifacts: &Value) -> Result<Plan, String> {
        let order = artifacts.get("order").ok_or("the project has no order of its ingest steps (artifacts.order)")?;
        Plan::of_value(order)
    }

    pub fn of_value(order: &Value) -> Result<Plan, String> {
        let stages = order.as_array().ok_or("artifacts.order is not a list of stages")?;
        let mut plan = Plan { stages: Vec::new() };
        for stage in stages {
            let codes = stage.as_array().ok_or("a stage of artifacts.order is not a list of steps")?;
            let mut steps = Vec::new();
            for code in codes {
                let code = code.as_str().ok_or("a step of artifacts.order is not a name")?;
                let step = Step::of(code).ok_or_else(|| format!("artifacts.order: no step {code}"))?;
                if plan.has(step) || steps.contains(&step) {
                    return Err(format!("artifacts.order: {code} twice"));
                }
                steps.push(step);
            }
            if steps.is_empty() {
                return Err("artifacts.order: an empty stage".into());
            }
            plan.stages.push(steps);
        }
        plan.check()?;
        Ok(plan)
    }

    pub fn default_plan() -> Plan {
        Plan::of_value(&serde_json::from_str(DEFAULT).expect("the default order is JSON")).expect("the default order is a plan")
    }

    /// Whether the plan has `step` at all (a template may leave a step out).
    pub fn has(&self, step: Step) -> bool {
        self.stage_of(step).is_some()
    }

    /// The stage `step` is in.
    pub fn stage_of(&self, step: Step) -> Option<usize> {
        self.stages.iter().position(|stage| stage.contains(&step))
    }

    /// Every step of the plan, stage by stage.
    pub fn steps(&self) -> impl Iterator<Item = Step> + '_ {
        self.stages.iter().flatten().copied()
    }

    /// Every step after the steps it works from.
    fn check(&self) -> Result<(), String> {
        for (at, stage) in self.stages.iter().enumerate() {
            for step in stage {
                for need in step.needs() {
                    match self.stage_of(*need) {
                        Some(need_at) if need_at <= at => {}
                        Some(_) => return Err(format!("artifacts.order: {} comes before {}", step.code(), need.code())),
                        None => return Err(format!("artifacts.order: {} needs {}", step.code(), need.code())),
                    }
                }
            }
        }
        Ok(())
    }

    /// The plan as the setting writes it.
    pub fn to_value(&self) -> Value {
        Value::Array(self.stages.iter().map(|stage| Value::Array(stage.iter().map(|step| Value::String(step.code().into())).collect())).collect())
    }

    /// One line for the log.
    pub fn line(&self) -> String {
        self.stages.iter().map(|stage| stage.iter().map(|step| step.code()).collect::<Vec<_>>().join("+")).collect::<Vec<_>>().join(" > ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_default_order_is_a_plan_and_written_back_the_same() {
        let plan = Plan::default_plan();
        assert_eq!(plan.stages.len(), 3);
        assert_eq!(Plan::of_value(&plan.to_value()).unwrap(), plan);
        assert_eq!(plan.line(), "import+posters > poster_look+waves+filmstrips+transcripts > shots+content+captions");
        assert_eq!(plan.steps().count(), 9);
    }

    #[test]
    fn a_step_before_what_it_works_from_or_twice_or_unknown_is_refused() {
        assert!(Plan::of_value(&json!([["waves"], ["import"]])).is_err());
        assert!(Plan::of_value(&json!([["import", "waves", "import"]])).is_err());
        assert!(Plan::of_value(&json!([["import"], ["teleport"]])).is_err());
        assert!(Plan::of_value(&json!([["content"]])).is_err(), "content without shots");
        let import_only = Plan::of_value(&json!([["import"]])).unwrap();
        assert!(!import_only.has(Step::Waves), "a template may leave a step out");
    }

    #[test]
    fn a_project_without_its_order_is_an_error_not_a_default() {
        assert!(Plan::of_settings(&json!({"filmstrip": "auto"})).is_err());
        assert!(Plan::of_settings(&json!({"order": serde_json::from_str::<Value>(DEFAULT).unwrap()})).is_ok());
    }
}
