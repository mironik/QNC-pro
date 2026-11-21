use super::*;

#[test]
fn the_system_templates_are_seeded_a_user_template_is_made_selected_and_removed_a_system_one_stays() {
    let root = std::env::temp_dir().join(format!("qnc-project-templates-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let writer = writer(&root).unwrap();
    let reader = qnc_registry::reader(&root).unwrap();
    let rows = list(&reader).unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.template_id.as_str()).collect();
    assert!(ids.contains(&"tpl_breaking_news") && ids.contains(&"tpl_news_package"), "{ids:?}");
    assert!(rows.iter().all(|r| r.system));
    assert_eq!(rows.iter().filter(|r| r.selected).count(), 1, "the first template is selected until one is chosen");
    assert_eq!(selected(&reader).unwrap().template_id, selected_id(&reader).unwrap());
    assert!(get(&reader, "tpl_breaking_news").unwrap().unwrap().settings["playback"]["input"].is_string());
    select(&writer, &reader, "tpl_news_package").unwrap();
    assert_eq!(selected_id(&reader).unwrap(), "tpl_news_package");
    assert!(select(&writer, &reader, "tpl_nothing").is_err());
    let mut settings = get(&reader, "tpl_news_package").unwrap().unwrap().settings;
    settings["products"] = serde_json::json!({ "thumbnails": "products/thumbnails" });
    let user = create_user(&writer, &reader, "Ingest ALL", "sve", "", Some(&settings)).unwrap();
    assert!(user.template_id.starts_with("tpl_user_ingest_all_"));
    assert_eq!(selected_id(&reader).unwrap(), user.template_id);
    let kept = get(&reader, &user.template_id).unwrap().unwrap();
    assert!(kept.settings.get("products").is_none(), "a template names no locations of a project");
    assert!(!kept.system);
    assert!(delete_user(&writer, &reader, "tpl_breaking_news").is_err(), "a system template is kept");
    delete_user(&writer, &reader, &user.template_id).unwrap();
    assert!(get(&reader, &user.template_id).unwrap().is_none());
    assert_eq!(selected_id(&reader).unwrap(), list(&reader).unwrap()[0].template_id, "the first template again");
    drop(reader);
    drop(writer);
    let _ = std::fs::remove_dir_all(&root);
}
