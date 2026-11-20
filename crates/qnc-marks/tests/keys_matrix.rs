//! The golden table of the marks: every action of the catalog that touches them, in every
//! state, with what it does and what the marks are afterwards. A changed line is a changed
//! behaviour and needs the user's decision. Rewrite with `QNC_WRITE_GOLDEN=1`.

use qnc_marks::{Done, Marks, Taken};

const ACTIONS: [&str; 12] = [
    "mark_in",
    "mark_out",
    "select_mark_in",
    "select_mark_out",
    "step_back_frame",
    "step_forward_frame",
    "activate_focused_item",
    "clear_focus",
    "undo_object",
    "go_to_mark_in",
    "navigate_prev_object",
    "navigate_next_object",
];

fn states() -> Vec<(&'static str, Marks)> {
    let fresh = Marks::of_clip(100);
    let saved = Marks::with(100, Some(10), Some(40));
    let mut in_taken = saved.clone();
    in_taken.act("select_mark_in", 20);
    in_taken.act("step_back_frame", 20);
    let mut out_taken = saved.clone();
    out_taken.act("select_mark_out", 20);
    out_taken.act("step_forward_frame", 20);
    let mut at_edges = Marks::with(100, Some(39), Some(40));
    at_edges.act("select_mark_in", 0);
    vec![
        ("fresh clip, nothing set", fresh),
        ("IN 10 OUT 40, nothing taken", saved),
        ("IN taken and moved to 9", in_taken),
        ("OUT taken and moved to 41", out_taken),
        ("IN 39 taken against OUT 40", at_edges),
    ]
}

fn show(marks: &Marks) -> String {
    let taken = match marks.taken() {
        Taken::None => "-",
        Taken::In => "IN",
        Taken::Out => "OUT",
    };
    format!("in={:?} out={:?} taken={taken}", marks.set_in(), marks.set_out())
}

fn table() -> String {
    let mut lines = Vec::new();
    for (name, state) in states() {
        for action in ACTIONS {
            let mut marks = state.clone();
            let done = marks.act(action, 20);
            lines.push(format!("{name} | playhead 20 | {action} -> {done:?} | {}", show(&marks)));
        }
        let mut marks = state.clone();
        let done = marks.click();
        lines.push(format!("{name} | click -> {done:?} | {}", show(&marks)));
        let mut marks = state.clone();
        let done = marks.drag(Taken::In, 25);
        lines.push(format!("{name} | drag IN to 25 -> {done:?} | {}", show(&marks)));
    }
    lines.join("\n") + "\n"
}

#[test]
fn the_golden_table_of_the_marks_is_unchanged() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("keys_matrix.golden");
    let now = table();
    if std::env::var_os("QNC_WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &now).unwrap();
    }
    let golden = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(now, golden, "the marks behave differently; a changed line needs the user's decision");
}

#[test]
fn enter_keeps_a_moved_mark_and_escape_puts_it_back_undo_puts_it_back_still_taken() {
    let mut marks = Marks::with(100, Some(10), Some(40));
    assert_eq!(marks.act("select_mark_in", 0), Done::Cue(10));
    assert_eq!(marks.act("step_forward_frame", 0), Done::Cue(11));
    assert_eq!(marks.act("step_forward_frame", 0), Done::Cue(12));
    assert_eq!(marks.act("activate_focused_item", 0), Done::Changed);
    assert_eq!((marks.set_in(), marks.taken()), (Some(12), Taken::None), "Enter keeps, never reverts");

    assert_eq!(marks.act("select_mark_out", 0), Done::Cue(39));
    assert_eq!(marks.act("step_back_frame", 0), Done::Cue(38));
    assert_eq!(marks.act("undo_object", 0), Done::Cue(39));
    assert_eq!((marks.set_out(), marks.taken()), (Some(40), Taken::Out), "Undo goes back, still taken");
    assert_eq!(marks.act("step_back_frame", 0), Done::Cue(38));
    assert_eq!(marks.act("clear_focus", 0), Done::Cue(39));
    assert_eq!((marks.set_out(), marks.taken()), (Some(40), Taken::None), "Esc goes back and lets go");

    assert_eq!(marks.act("select_mark_in", 0), Done::Cue(12));
    assert_eq!(marks.act("step_back_frame", 0), Done::Cue(11));
    assert_eq!(marks.click(), Done::Changed);
    assert_eq!((marks.set_in(), marks.taken()), (Some(11), Taken::None), "a click keeps, as Enter");
}

#[test]
fn every_clip_is_virtual_and_marks_never_cross_or_leave_the_clip() {
    let mut marks = Marks::of_clip(100);
    assert_eq!((marks.in_(), marks.out()), (0, 100));
    assert_eq!(marks.act("select_mark_out", 0), Done::Cue(99), "OUT of a fresh clip is its end");
    assert_eq!(marks.act("step_forward_frame", 0), Done::Refused, "out of the clip");
    assert_eq!(marks.act("step_back_frame", 0), Done::Cue(98));
    assert_eq!(marks.set_out(), Some(99), "moving the clip's end sets OUT");
    assert_eq!(marks.act("clear_focus", 0), Done::Cue(99));
    assert_eq!(marks.set_out(), None, "Esc: the clip's end again");
    assert_eq!(marks.act("mark_in", 50), Done::Changed);
    assert_eq!(marks.act("mark_out", 30), Done::Changed);
    assert_eq!((marks.in_(), marks.out()), (50, 51), "OUT never before IN: a frame after it");
    assert_eq!(marks.drag(Taken::In, 90), Done::Cue(50), "IN stops a picture before OUT");
    assert_eq!(marks.act("mark_out", 500), Done::Changed);
    assert_eq!(marks.out(), 100, "OUT stops at the clip's end");
}
