use crate::*;

/// A final record of a real clip (Sony FX6 card, two representations), saved by the old QNC.
const SAVED: &str = include_str!("../tests/snapshot.json");

fn saved() -> Snapshot {
    serde_json::from_str(SAVED).expect("the saved record reads in the old shape")
}

#[test]
fn a_record_the_old_qnc_saved_reads_and_its_report_is_what_the_facts_give() {
    let snapshot = saved();
    assert_eq!(snapshot.phase, Phase::Final);
    assert_eq!(snapshot.report, inspect(&snapshot.metadata), "the saved report equals the rule here");
    assert_eq!(snapshot.completeness, completeness(&snapshot.report));
    snapshot.validate().unwrap();
    let text = serde_json::to_string(&snapshot).unwrap();
    let again: Snapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(again, snapshot, "written back the same");
    assert!(snapshot.report.missing_under("original").is_empty(), "the original plays: nothing missing under it");
}

#[test]
fn a_record_whose_report_does_not_match_its_facts_or_whose_phase_lies_is_refused() {
    let mut snapshot = saved();
    snapshot.report.issues.clear();
    snapshot.completeness = Completeness::Complete;
    assert_eq!(snapshot.validate(), Err(Error::Protocol));
    let mut camera = saved();
    camera.phase = Phase::Camera;
    camera.revision = 1;
    assert_eq!(camera.validate(), Err(Error::InvalidMetadata), "a camera record may not carry a probe");
    let mut unbound = saved();
    unbound.binding.original_uri = "qnc://local/source/x/file/other".into();
    assert_eq!(unbound.validate(), Err(Error::InvalidMetadata));
}
