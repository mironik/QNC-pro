use super::*;

fn project() -> (tempfile::TempDir, Writer) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("project.db");
    qnc_db::create(&file).unwrap();
    let writer = Writer::start(&file, vec![Box::new(StorySheetTables)]).unwrap();
    (dir, writer)
}

fn sheet(slug: &str) -> StorySheet {
    StorySheet {
        sheet: serde_json::json!({ "sheet_id": "news", "fields": [{ "id": "slug", "label": "Slug" }] }),
        values: [("slug".to_string(), Value::String(slug.into())), ("target_seconds".to_string(), Value::from(90))].into(),
    }
}

#[test]
fn a_project_without_a_sheet_reads_an_empty_one_and_a_saved_sheet_replaces_the_last() {
    let (dir, writer) = project();
    let reader = Reader::open(&dir.path().join("project.db")).unwrap();
    assert_eq!(read(&reader).unwrap(), StorySheet::default());
    save(&writer, sheet("PROTEST")).unwrap();
    let mut again = sheet("PROTEST_SPLIT");
    again.values.remove("target_seconds");
    save(&writer, again.clone()).unwrap();
    assert_eq!(read(&reader).unwrap(), again);
    assert_eq!(again.shown(), vec![("Slug".to_string(), "PROTEST_SPLIT".to_string())]);
    assert_eq!(again.search_text(), "PROTEST_SPLIT");
}

#[test]
fn a_bad_code_or_a_sheet_that_is_no_object_keeps_nothing() {
    let (dir, writer) = project();
    let reader = Reader::open(&dir.path().join("project.db")).unwrap();
    let mut bad = sheet("X");
    bad.values.insert("Slug Name".into(), Value::Null);
    assert!(save(&writer, bad).is_err());
    assert!(save(&writer, StorySheet { sheet: Value::from(3), values: BTreeMap::new() }).is_err());
    assert_eq!(read(&reader).unwrap(), StorySheet::default());
}
