use super::*;

fn fixture(uri: &str) -> Catalog {
    let applications = [("first", "a"), ("lite", "c"), ("full", "c"), ("last", "d")]
        .into_iter()
        .map(|(id, group)| Application {
            application_id: format!("qnc.{id}"),
            tab_id: id.into(),
            label: id.into(),
            priority_group: group.into(),
            system: false,
            removable: true,
            host_mode: HostMode::ExternalComponent,
            desktop_entry: id.into(),
            standalone_executable: id.into(),
        })
        .collect();
    Catalog {
        catalog_id: CATALOG_ID.into(),
        schema_version: 1,
        catalog_uri: uri.into(),
        selection_policy: "one_per_priority_group".into(),
        observed_at_unix_ms: 1,
        target_os: "linux".into(),
        target_cpu: "aarch64".into(),
        applications,
        unavailable: vec![],
    }
}

#[test]
fn radio_groups_replace_one_variant_without_affecting_another_template() {
    let c = fixture(DEFAULT_URI);
    let mut one = vec![];
    let mut two = vec![];
    choose_group(&c, &mut one, "c", Some("qnc.lite")).unwrap();
    choose_group(&c, &mut two, "c", Some("qnc.lite")).unwrap();
    choose_group(&c, &mut one, "c", Some("qnc.full")).unwrap();
    assert_eq!(one, ["qnc.full"]);
    assert_eq!(two, ["qnc.lite"]);
    let g = groups(&c, &one);
    assert_eq!(g[1].choices.iter().filter(|c| c.selected).count(), 1);
    assert!(choose_group(&c, &mut one, "c", Some("qnc.first")).is_err());
    assert!(choose_group(&c, &mut one, "c", Some("qnc.missing")).is_err());
    assert_eq!(one, ["qnc.full"]);
    choose_group(&c, &mut one, "c", None).unwrap();
    assert!(one.is_empty());
    assert!(groups(&c, &one)[1].no_selection);
}

#[test]
fn snapshot_sorts_gaps_and_rejects_forged_group_order() {
    let c = fixture(DEFAULT_URI);
    let ids = ["qnc.last", "qnc.first", "qnc.full"].map(str::to_owned);
    let mut selection = SelectionSnapshot::from_catalog(&c, &ids).unwrap();
    assert_eq!(
        selection
            .applications
            .iter()
            .map(|app| app.priority_group.as_str())
            .collect::<Vec<_>>(),
        ["a", "c", "d"]
    );
    selection.validate().unwrap();
    selection.applications.swap(0, 2);
    assert!(selection.validate().is_err());
    selection.applications.swap(0, 2);
    selection.applications[2].priority_group = "c".into();
    assert!(selection.validate().is_err());
}

#[test]
fn missing_duplicate_and_unavailable_selections_are_rejected() {
    let c = fixture(DEFAULT_URI);
    for ids in [
        vec!["qnc.missing"],
        vec!["qnc.first", "qnc.first"],
        vec!["qnc.lite", "qnc.full"],
    ] {
        assert!(SelectionSnapshot::from_catalog(
            &c,
            &ids.into_iter().map(str::to_owned).collect::<Vec<_>>()
        )
        .is_err());
    }
}


#[test]
fn a_catalog_file_is_read_and_a_missing_one_makes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("catalog.json");
    assert!(read_catalog(&path).is_err());
    assert!(!path.exists());
    let bytes = serde_json::to_vec(&fixture(DEFAULT_URI)).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(read_catalog(&path).unwrap().applications.len(), 4);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::write(&path, serde_json::to_vec(&fixture("qnc://local/catalog/other")).unwrap()).unwrap();
    assert!(read_catalog(&path).is_err(), "another catalog is refused");
}
