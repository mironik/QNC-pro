use std::path::Path;
use std::sync::atomic::AtomicBool;

use qnc_card_select::Card;

use crate::*;

const SEED: &str = include_str!("../../../seed/system_seed.json");

fn profiles() -> Vec<qnc_camera_profile::Profile> {
    qnc_camera_profile::read_profiles(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/cameras/profiles")).0
}

fn fresh_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("qnc-ingest-run-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

#[test]
fn a_card_without_its_media_publishes_its_clips_and_marks_the_copies_failed_with_the_reason() {
    let root = fresh_root("fixture");
    let disk = root.join("card");
    let card_root = disk.join("PRIVATE").join("XDROOT");
    std::fs::create_dir_all(card_root.join("Clip")).unwrap();
    std::fs::write(card_root.join("MEDIAPRO.XML"), include_str!("../../qnc-sony-index/tests/MEDIAPRO.XML")).unwrap();
    std::fs::write(card_root.join("Clip").join("Mironik 1483M01.XML"), include_str!("../../qnc-sony-index/tests/Mironik_1483M01.XML")).unwrap();
    let template: serde_json::Value = serde_json::from_str::<serde_json::Value>(SEED).unwrap()["project_templates"][2]["settings"].clone();
    let made = qnc_project_make::make(&root, qnc_project_make::Make { name: "Fixture".into(), settings: Some(template), projects_root: Some(root.join("projects")), ..Default::default() }).unwrap();
    let project = Project::open(&made.folder).unwrap();
    let card = Card { disk: disk.clone(), serial: "67870D0C".into(), volume_name: "T".into(), root_path: "PRIVATE/XDROOT".into() };
    let selected = select_into(&card, &project, &profiles(), 7).unwrap();
    assert_eq!(selected.clips.len(), 1);
    let reader = Reader::open(&made.project_db()).unwrap();
    assert_eq!(qnc_catalog::summaries(&reader).unwrap().len(), 1);
    let mut seen = Vec::new();
    let progress = run(&card, &project, &selected, &AtomicBool::new(false), 8, &mut |p| seen.push(p)).unwrap();
    assert_eq!((progress.clips, progress.failed, progress.finished), (1, 1, 0));
    let stored = qnc_catalog::read_clip(&reader, selected.clips[0].id()).unwrap().unwrap();
    assert_eq!(stored.import_status, ImportStatus::Failed);
    assert!(stored.import_error.unwrap().contains("Mironik 1483.MXF"));
    assert!(!is_done(&reader, selected.clips[0].id()).unwrap());
    drop(project);
    drop(reader);
    let _ = std::fs::remove_dir_all(&root);
}

/// The real card on D: (read only): two clips copied into a fresh project, probed, final, and
/// the player input prepared from the catalog alone. Skipped when the card is not in.
#[test]
fn two_clips_of_the_real_card_become_final_imported_clips_the_player_input_takes() {
    let disk = Path::new("D:\\");
    if !disk.join("PRIVATE").join("XDROOT").join("MEDIAPRO.XML").is_file() || qnc_launch_present().is_err() {
        eprintln!("skipped: no card on D: or no ffprobe beside the build");
        return;
    }
    let root = fresh_root("card");
    let template: serde_json::Value = serde_json::from_str::<serde_json::Value>(SEED).unwrap()["project_templates"][2]["settings"].clone();
    let made = qnc_project_make::make(&root, qnc_project_make::Make { name: "Card test".into(), settings: Some(template), projects_root: Some(root.join("projects")), ..Default::default() }).unwrap();
    let project = Project::open(&made.folder).unwrap();
    let card = Card { disk: disk.into(), serial: "67870D0C".into(), volume_name: "UNTITLED".into(), root_path: "PRIVATE/XDROOT".into() };
    let mut selected = select_into(&card, &project, &profiles(), 9).unwrap();
    assert!(selected.clips.len() >= 100);
    selected.clips.retain(|c| c.name == "Mironik 1483.MXF" || c.name == "Mironik 1484.MXF");
    let progress = run(&card, &project, &selected, &AtomicBool::new(false), 10, &mut |_| {}).unwrap();
    assert_eq!((progress.clips, progress.finished, progress.failed), (2, 2, 0), "{progress:?}");
    let reader = Reader::open(&made.project_db()).unwrap();
    let settings = qnc_project_settings::read(&reader).unwrap().playback().unwrap();
    for clip in &selected.clips {
        assert!(is_done(&reader, clip.id()).unwrap());
        let stored = qnc_catalog::read_clip(&reader, clip.id()).unwrap().unwrap();
        assert!(stored.clip.snapshot.binding.original_uri.starts_with(&format!("qnc://local/source/project-{}/file/original/PRIVATE/XDROOT/Clip/", made.project_id)));
        assert!(stored.clip.thumbnail_uri.as_deref().unwrap().contains("/products/thumbnails/"));
        let prepared = qnc_player_input::prepare(&settings, &stored.for_player()).unwrap();
        assert_eq!(prepared.representation, qnc_player_input::Representation::Original);
        assert!(prepared.layout.video.as_ref().unwrap().duration_frames > 0);
        let copy = made.folder.join("original").join("PRIVATE").join("XDROOT").join("Clip").join(&clip.name);
        assert!(copy.is_file());
    }
    drop(project);
    drop(reader);
    let _ = std::fs::remove_dir_all(&root);
}

fn qnc_launch_present() -> Result<(), String> {
    qnc_probe::probe(Path::new("C:/nowhere.mxf"), "qnc://local/source/x/file/y", Duration::from_secs(1)).map(|_| ()).and_then(|_| Ok(())).or_else(|e| if e.contains("not beside") { Err(e) } else { Ok(()) })
}
