//! The bars of the background work as the ingest window shows them (copied from the old QNC's
//! `qnc-work-progress-view`): one row per function, its name, its bar with done of all, the time
//! and an X that stops the function when the user does not need it. It draws what it is given
//! and says which X was pressed; the caller stops the function.

use std::time::Duration;

use eframe::egui::{self, Color32};
use qnc_form_look::{FormLook, Grows};

/// One bar: the code of its name, what its X stops, how many of all are done, whether it is stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    pub text: &'static str,
    pub function: &'static str,
    pub done: usize,
    pub stopped: bool,
}

/// Where the columns of a row are and how big its parts are, from the form's look: the name and
/// the start of the track from the row's left edge; the end of the track and the right edges of
/// the count, the time and the X from the row's right edge.
#[derive(Clone, Copy, Debug)]
pub struct BarsLook {
    pub row: f32,
    pub bar: f32,
    pub x: f32,
    pub name_at: f32,
    pub track_from: f32,
    pub track_to: f32,
    pub count_at: f32,
    pub time_at: f32,
    pub x_at: f32,
}

impl BarsLook {
    /// The measures from a look file (`bar_row`, `bar_thickness`, `bar_x`, `bar_name_at`,
    /// `bar_track_from`, `bar_track_to`, `bar_count_at`, `bar_time_at`, `bar_x_at`).
    pub fn of(look: &FormLook) -> Self {
        Self {
            row: look.get("bar_row"),
            bar: look.get("bar_thickness"),
            x: look.get("bar_x"),
            name_at: look.get("bar_name_at"),
            track_from: look.get("bar_track_from"),
            track_to: look.get("bar_track_to"),
            count_at: look.get("bar_count_at"),
            time_at: look.get("bar_time_at"),
            x_at: look.get("bar_x_at"),
        }
    }
}

/// A time as m:ss (h:mm:ss from an hour on).
pub fn clock(time: Duration) -> String {
    let seconds = time.as_secs();
    match seconds / 3600 {
        0 => format!("{}:{:02}", seconds / 60, seconds % 60),
        hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
    }
}

/// Draws `bars` of `clips` clips in colour `ink` at the places of `look`; `time_of` gives the
/// time beside a bar. Gives the function whose X was pressed.
pub fn bars(ui: &mut egui::Ui, bars: &[Bar], clips: usize, ink: Color32, time_of: &dyn Fn(&Bar) -> Option<Duration>, look: &BarsLook) -> Option<&'static str> {
    let mut stop = None;
    let total = clips.max(1) as f32;
    let (muted, track, done_fill) = (ui.visuals().weak_text_color(), ui.visuals().extreme_bg_color, ui.visuals().selection.bg_fill);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = ui.available_width();
    for bar in bars {
        let (row, _) = ui.allocate_exact_size(egui::vec2(width, look.row), egui::Sense::hover());
        let painter = ui.painter_at(row);
        let mid = row.center().y;
        let x_rect = egui::Rect::from_center_size(egui::pos2(row.right() - look.x_at - look.x / 2.0, mid), egui::vec2(look.x, look.x));
        let bar_left = row.left() + look.track_from;
        let bar_right = (row.right() - look.track_to).max(bar_left + 10.0);
        painter.text(egui::pos2(row.left() + look.name_at, mid), egui::Align2::LEFT_CENTER, qnc_text::text(bar.text), font.clone(), ink);
        let track_rect = egui::Rect::from_min_max(egui::pos2(bar_left, mid - look.bar / 2.0), egui::pos2(bar_right, mid + look.bar / 2.0));
        painter.rect_filled(track_rect, look.bar / 2.0, track);
        let share = (bar.done as f32 / total).clamp(0.0, 1.0);
        if share > 0.0 {
            let filled = egui::Rect::from_min_max(track_rect.min, egui::pos2(track_rect.left() + track_rect.width() * share, track_rect.bottom()));
            painter.rect_filled(filled, look.bar / 2.0, if bar.stopped { muted } else { done_fill });
        }
        let count = if bar.stopped { qnc_text::fill("ingest.stopped", &[("done", &bar.done.to_string()), ("clips", &clips.to_string())]) } else { format!("{} / {}", bar.done, clips) };
        painter.text(egui::pos2(row.right() - look.count_at, mid), egui::Align2::RIGHT_CENTER, count, font.clone(), ink);
        if let Some(time) = time_of(bar) {
            painter.text(egui::pos2(row.right() - look.time_at, mid), egui::Align2::RIGHT_CENTER, clock(time), font.clone(), muted);
        }
        if !bar.stopped && bar.done < clips {
            let x = ui.interact(x_rect, ui.id().with(("qnc-work-stop", bar.function)), egui::Sense::click()).on_hover_text(qnc_text::text("ingest.stop_hint"));
            let colour = if x.hovered() { ink } else { muted };
            if x.hovered() {
                painter.rect_filled(x_rect, 4.0, track);
            }
            painter.text(x_rect.center(), egui::Align2::CENTER_CENTER, "×", font.clone(), colour);
            if x.clicked() {
                stop = Some(bar.function);
            }
        }
    }
    stop
}

/// As [`bars`], with the measures of the form's look, each column of the first row marked so it
/// can be moved on the form.
pub fn bars_marked(ui: &mut egui::Ui, rows: &[Bar], clips: usize, ink: Color32, time_of: &dyn Fn(&Bar) -> Option<Duration>, look: &FormLook) -> Option<&'static str> {
    let m = BarsLook::of(look);
    let (left, right, top) = (ui.cursor().left(), ui.cursor().left() + ui.available_width(), ui.cursor().top());
    let stop = bars(ui, rows, clips, ink, time_of, &m);
    if look.editing() && !rows.is_empty() {
        let (mid, edge) = (top + m.row / 2.0, 4.0);
        let span = |from: f32, to: f32| egui::Rect::from_x_y_ranges(from..=to, top..=top + m.row);
        let (track_left, track_right) = (left + m.track_from, right - m.track_to);
        look.mark(ui, egui::Rect::from_x_y_ranges(track_left + edge..=track_right - edge, mid - m.bar / 2.0..=mid + m.bar / 2.0), "bar_thickness", Grows::Down);
        look.mark(ui, span(left + m.name_at, (left + m.name_at + 60.0).min(track_left - edge)), "bar_name_at", Grows::Right);
        look.mark(ui, span(track_left - edge, track_left + edge), "bar_track_from", Grows::Right);
        look.mark(ui, span(track_right - edge, track_right + edge), "bar_track_to", Grows::Left);
        look.mark(ui, span(right - m.count_at - 50.0, right - m.count_at), "bar_count_at", Grows::Left);
        look.mark(ui, span(right - m.time_at - 36.0, right - m.time_at), "bar_time_at", Grows::Left);
        look.mark(ui, span(right - m.x_at - m.x, right - m.x_at), "bar_x_at", Grows::Left);
        look.mark(ui, egui::Rect::from_x_y_ranges(left..=right, top + m.row - 3.0..=top + m.row), "bar_row", Grows::Down);
    }
    stop
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_clock_reads_minutes_and_hours() {
        assert_eq!(super::clock(std::time::Duration::from_secs(65)), "1:05");
        assert_eq!(super::clock(std::time::Duration::from_secs(3725)), "1:02:05");
    }
}
