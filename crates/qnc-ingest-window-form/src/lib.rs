//! The ingest window form (copied from the old QNC's `qnc-ai-card-ingest`, its drawing): the
//! ingest of one card in its own window with QNC's title bar. The title says where the ingest is,
//! under it the card and the time; a card "Background work" with the clips and the bars of its
//! functions (an X stops one); a card "Project" with the project's name, its place and description
//! to type, and Cancel on its title line. The questions (Cancel? keep or delete the partial
//! import?) are QNC's confirm dialog. Every measure is in the window's look file. The session
//! (`qnc-ingest-window`) does the work; this form draws its view and hands back the answers.

use eframe::egui::{self, RichText};
use qnc_desktop_theme::Theme;
use qnc_form_look::{FormLook, Grows};
use qnc_ingest_window::{Ask, Phase, Session, View};
use qnc_text::{fill, text};
use qnc_window_host::Next;

/// The measures this window asks of its look file.
const LOOK_KEYS: &[&str] = &[
    "window_width", "window_height", "margin_left", "margin_right", "margin_top", "margin_bottom", "title_size", "subtitle_size", "header_gap", "card_gap", "card_padding", "card_corner",
    "card_frame", "card_title_size", "card_title_gap", "line_gap", "bars_gap", "after_bars_gap", "bar_row", "bar_name_at", "bar_track_from", "bar_track_to", "bar_count_at", "bar_time_at",
    "bar_x_at", "bar_x", "bar_thickness", "field_label_size", "field_gap", "cancel_width",
];

/// Runs the ingest window until the ingest is cancelled or the user closes it.
pub fn main() -> Result<(), String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-ingest-window");
    qnc_diagnostics::log_panics();
    qnc_settings::choose_language(&root)?;
    let args: Vec<String> = std::env::args().collect();
    let mut session = Session::start(&root, &args)?;
    let mut look = FormLook::load(&root, "ingest-window", LOOK_KEYS)?;
    let theme = Theme::current();
    let mut front = qnc_window_front::WindowFront::default();
    let mut brought = false;
    let size = [look.get("window_width"), look.get("window_height")];
    qnc_window_host::run_framed(text("ingest.window_title"), size, true, move |ctx, ui| {
        look.editor(ctx);
        look.window(ctx, qnc_title_bar::HEIGHT);
        let view = session.poll();
        if view.close {
            return Next::Close;
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            session.closing();
        }
        // The end is seen even when the user worked on something else meanwhile.
        if view.phase == Phase::Done && !brought {
            brought = true;
            front.bring(ctx);
        }
        front.follow(ctx);
        body(ui, &theme, &look, &view, &mut session);
        questions(ctx, &view, &mut session);
        if let Some((notice, left)) = &view.notice {
            egui::Area::new(egui::Id::new("card_out_notice")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(24)).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(notice).size(20.0).strong());
                        ui.add_space(8.0);
                        ui.label(fill("ingest.window_closes_in", &[("seconds", &left.to_string())]));
                    });
                });
            });
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        Next::Show
    })
}

/// A card of the window: a slightly raised surface with a fine frame and round corners, its
/// title in small capitals; `button` (its text and size) stands at the right end of the title's
/// line. Gives whether the button was pressed.
fn card(ui: &mut egui::Ui, theme: &Theme, look: &FormLook, title: &str, button: Option<(&str, egui::Vec2)>, contents: impl FnOnce(&mut egui::Ui)) -> bool {
    let padding = look.get("card_padding");
    let mut pressed = false;
    let shown = egui::Frame::new()
        .fill(theme.surface)
        .stroke(egui::Stroke::new(look.get("card_frame"), theme.border))
        .corner_radius(look.get("card_corner"))
        .inner_margin(egui::Margin::same(padding.round() as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let visuals = ui.visuals_mut();
            visuals.extreme_bg_color = theme.bg;
            visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme.border);
            visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme.text_muted);
            let title = ui
                .horizontal(|ui| {
                    let title = ui.label(RichText::new(title.to_uppercase()).size(theme.font_ui + look.get("card_title_size")).color(theme.text_muted).strong());
                    if let Some((label, size)) = button {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            pressed = ui.add(qnc_ui_kit::sized_button(label, size)).clicked();
                        });
                    }
                    title
                })
                .inner;
            look.mark(ui, title.rect, "card_title_size", Grows::Up);
            look.space(ui, "card_title_gap");
            ui.spacing_mut().item_spacing.y = look.get("line_gap");
            contents(ui);
        });
    let rect = shown.response.rect;
    look.mark(ui, egui::Rect::from_x_y_ranges(rect.left()..=rect.left() + padding, rect.y_range()), "card_padding", Grows::Right);
    pressed
}

fn body(ui: &mut egui::Ui, theme: &Theme, look: &FormLook, view: &View, session: &mut Session) {
    let title = match view.phase {
        Phase::Running => text("ingest.running"),
        Phase::Done => text("ingest.done"),
        Phase::Cancelled => text("ingest.cancelled"),
    };
    egui::Frame::NONE.fill(theme.bg).inner_margin(egui::Margin { left: look.get("margin_left") as i8, right: look.get("margin_right") as i8, top: look.get("margin_top") as i8, bottom: look.get("margin_bottom") as i8 }).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let title = ui.label(RichText::new(title).size(theme.font_ui + look.get("title_size")).color(theme.text).strong());
            look.mark(ui, title.rect, "title_size", Grows::Up);
            let mut sub = format!("{}   ·   {}", view.card, fill("ingest.elapsed", &[("time", &qnc_work_bars::clock(view.elapsed))]));
            if let Some(said) = &view.card_said {
                sub.push_str("   ·   ");
                sub.push_str(said);
            }
            let sub = ui.label(RichText::new(sub).size(theme.font_ui + look.get("subtitle_size")).color(theme.text_muted));
            look.mark(ui, sub.rect, "subtitle_size", Grows::Up);
            if let Some(known) = &view.known {
                ui.label(RichText::new(known).size(theme.font_ui + look.get("subtitle_size")).color(theme.text));
            }
            look.space(ui, "header_gap");
            let mut stop = None;
            card(ui, theme, look, text("work.title"), None, |ui| {
                ui.label(RichText::new(fill("ingest.clips", &[("clips", &view.clips.to_string())])).color(theme.text_muted));
                look.space(ui, "bars_gap");
                let bars: Vec<qnc_work_bars::Bar> = view.bars.iter().map(|bar| qnc_work_bars::Bar { text: bar.text, function: bar.function, done: bar.done, stopped: bar.stopped }).collect();
                let time_of = |bar: &qnc_work_bars::Bar| session.time_of(&qnc_ingest_window::Bar { text: bar.text, function: bar.function, done: bar.done, stopped: bar.stopped });
                stop = qnc_work_bars::bars_marked(ui, &bars, view.clips, theme.text, &time_of, look);
                look.space(ui, "after_bars_gap");
                ui.label(RichText::new(fill("ingest.copied_mb", &[("mb", &(view.bytes_copied / 1_000_000).to_string())])).color(theme.text_muted));
                if view.failed > 0 {
                    ui.label(RichText::new(fill("ingest.failed", &[("failed", &view.failed.to_string())])).color(theme.danger));
                }
            });
            if let Some(function) = stop {
                session.stop_function(function);
            }
            look.space(ui, "card_gap");
            let cancel_size = egui::vec2(look.get("cancel_width"), theme.chrome_control_height);
            let show_cancel = view.phase == Phase::Running;
            let mut left = false;
            let cancel = card(ui, theme, look, text("ingest.project_card"), show_cancel.then_some((text("ingest.cancel"), cancel_size)), |ui| {
                ui.label(RichText::new(&view.project_name).color(theme.text).strong());
                let label = |ui: &mut egui::Ui, code: &'static str| ui.label(RichText::new(text(code)).size(theme.font_ui + look.get("field_label_size")).color(theme.text_muted));
                look.space(ui, "field_gap");
                label(ui, "ingest.location");
                let location = ui.add(qnc_ui_kit::text_field(&mut session.info.location).desired_width(f32::INFINITY).hint_text(text("ingest.location_hint")));
                look.space(ui, "field_gap");
                label(ui, "ingest.description");
                let description = ui.add(qnc_ui_kit::text_area(&mut session.info.description).desired_width(f32::INFINITY).desired_rows(2).hint_text(text("ingest.description_hint")));
                left = location.lost_focus() || description.lost_focus();
            });
            if left {
                session.keep_info();
            }
            if cancel {
                session.ask_cancel();
            }
        });
    });
}

/// The questions over the window: Cancel the ingest? Keep or delete the partial import?
fn questions(ctx: &egui::Context, view: &View, session: &mut Session) {
    let theme = qnc_theme::theme();
    match view.ask {
        Some(Ask::Cancel) => {
            let question = qnc_confirm::Question { id: "ingest_cancel", title: text("ingest.cancel_title"), subject: text("ingest.cancel_subject"), no: text("ingest.cancel_no"), yes: text("ingest.cancel_yes") };
            if let Some(yes) = qnc_confirm::show(ctx, theme, question) {
                session.answer_cancel(yes);
            }
        }
        Some(Ask::Partial) => {
            let question = qnc_confirm::Question { id: "ingest_partial", title: text("ingest.partial_title"), subject: text("ingest.partial_subject"), no: text("ingest.partial_keep"), yes: text("ingest.partial_delete") };
            if let Some(delete) = qnc_confirm::show(ctx, theme, question) {
                session.answer_partial(delete);
            }
        }
        None => {}
    }
}
