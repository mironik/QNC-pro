//! The panel of a running ingest: each card with the project it goes into, how far it is, what
//! failed, and Stop. Pure drawing: the lines are given, the answers returned.

use egui::{ProgressBar, RichText, Ui};
use qnc_theme::Theme;

/// One card as the panel shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLine {
    pub serial: String,
    pub name: String,
    pub project: Option<String>,
    pub clips: usize,
    pub finished: usize,
    pub failed: usize,
    pub bytes_copied: u64,
}

impl CardLine {
    fn share(&self) -> f32 {
        match self.clips {
            0 => 0.0,
            clips => (self.finished + self.failed) as f32 / clips as f32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    Stop(String),
    Hide,
}

/// Draws the cards; `waiting` is the line shown when there is none.
pub fn show(ui: &mut Ui, theme: &Theme, cards: &[CardLine], waiting: &str) -> Vec<Seen> {
    let mut seen = Vec::new();
    let text = qnc_ui_kit::colour(theme.colours.text);
    let muted = qnc_ui_kit::colour(theme.colours.muted);
    let danger = qnc_ui_kit::colour(theme.colours.danger);
    ui.heading(RichText::new(qnc_text::text("ingest.window_title")).color(text));
    ui.add_space(8.0);
    if cards.is_empty() {
        ui.label(RichText::new(waiting).color(muted));
    }
    for card in cards {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&card.name).color(text).strong());
            if let Some(project) = &card.project {
                ui.label(RichText::new(qnc_text::fill("ingest.project", &[("project", project)])).color(muted));
            }
        });
        ui.add(ProgressBar::new(card.share()).show_percentage());
        ui.horizontal(|ui| {
            ui.label(RichText::new(qnc_text::fill("ingest.line", &[("finished", &card.finished.to_string()), ("clips", &card.clips.to_string()), ("mb", &(card.bytes_copied / 1_000_000).to_string())])).color(muted));
            if card.failed > 0 {
                ui.label(RichText::new(qnc_text::fill("ingest.failed", &[("failed", &card.failed.to_string())])).color(danger));
            }
            if qnc_ui_kit::button(ui, theme, qnc_text::text("ingest.stop"), card.finished + card.failed < card.clips, false).clicked() {
                seen.push(Seen::Stop(card.serial.clone()));
            }
        });
        ui.add_space(6.0);
    }
    ui.add_space(8.0);
    if qnc_ui_kit::button(ui, theme, qnc_text::text("ingest.hide"), true, false).clicked() {
        seen.push(Seen::Hide);
    }
    seen
}
