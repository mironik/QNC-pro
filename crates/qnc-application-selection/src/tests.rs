use super::*;
use serde_json::json;

    fn ready() -> ApplicationSelection {
        let catalog: Catalog = serde_json::from_value(json!({
            "catalog_id": "qnc.catalog.applications", "schema_version": 1,
            "catalog_uri": catalog::DEFAULT_URI, "selection_policy": "one_per_priority_group",
            "observed_at_unix_ms": 1, "target_os": "windows", "target_cpu": "x86_64",
            "applications": [
                {"application_id":"qnc.first", "tab_id":"first", "label":"First", "priority_group":"a", "system":false,"removable":true,"host_mode":"external_component","desktop_entry":"first","standalone_executable":"first"},
                {"application_id":"qnc.lite", "tab_id":"lite", "label":"Lite", "priority_group":"b", "system":false,"removable":true,"host_mode":"external_component","desktop_entry":"lite","standalone_executable":"lite"},
                {"application_id":"qnc.full", "tab_id":"full", "label":"Full", "priority_group":"b", "system":false,"removable":true,"host_mode":"external_component","desktop_entry":"full","standalone_executable":"full"}
            ], "unavailable": []
        })).unwrap();
        ApplicationSelection {
            file: std::path::PathBuf::new(),
            pending: None,
            catalog: Some(catalog),
            ids: vec![],
            initial_settings: None,
            error: None,
        }
    }

    #[test]
    fn missing_apps_never_become_choices_or_block_creation_and_seed_is_unchanged() {
        let mut selection = ready();
        let settings = json!({"workspace":{"tabs":["first", "missing"]}});
        let before = settings.clone();
        selection.set_settings(&settings);
        assert_eq!(selection.view().groups.len(), 2);
        assert!(selection
            .view()
            .groups
            .iter()
            .flat_map(|group| &group.choices)
            .all(|choice| !choice.application_id.contains("missing")));
        assert_eq!(selection.snapshot().unwrap().applications.len(), 1);
        assert_eq!(settings, before);
    }

    #[test]
    fn radio_choice_replaces_same_group_and_template_switch_is_independent() {
        let mut selection = ready();
        selection.set_settings(&json!({"workspace":{"tabs":["first", "lite"]}}));
        selection.choose("b", Some("qnc.full")).unwrap();
        assert_eq!(
            selection.snapshot().unwrap().applications[1].application_id,
            "qnc.full"
        );
        selection.set_settings(&json!({"workspace":{"tabs":["lite"]}}));
        assert_eq!(
            selection.snapshot().unwrap().applications[1].application_id,
            "qnc.lite"
        );
        selection.choose("b", None).unwrap();
        assert!(selection.view().groups[1].no_selection);
        assert_eq!(selection.snapshot().unwrap().applications.len(), 1);
    }

    #[test]
    fn group_a_is_required_and_only_an_available_alternative_can_replace_it() {
        let mut selection = ready();
        selection.set_settings(&json!({"workspace":{"tabs":[]}}));
        assert_eq!(
            selection.snapshot().unwrap().applications[0].application_id,
            "qnc.first"
        );
        assert!(selection.choose("a", None).is_err());
        assert!(selection.choose("a", Some("qnc.missing")).is_err());
        let mut alternative = selection.catalog.as_ref().unwrap().applications[0].clone();
        alternative.application_id = "qnc.alternative".into();
        alternative.tab_id = "alternative".into();
        alternative.label = "Alternative".into();
        selection
            .catalog
            .as_mut()
            .unwrap()
            .applications
            .insert(1, alternative);
        selection.choose("a", Some("qnc.alternative")).unwrap();
        assert_eq!(
            selection.snapshot().unwrap().applications[0].application_id,
            "qnc.alternative"
        );
        assert_eq!(selection.snapshot().unwrap().applications.len(), 1);
        assert!(selection.choose("a", None).is_err());
    }

    #[test]
    fn no_catalog_means_no_choices_and_no_save() {
        let mut selection = ready();
        selection.catalog = None;
        selection.set_settings(&json!({"workspace":{"tabs":["first"]}}));
        assert!(selection.view().groups.is_empty());
        assert!(selection.snapshot().is_err());
    }
