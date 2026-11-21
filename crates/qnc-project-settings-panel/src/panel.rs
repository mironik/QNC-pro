//! The settings panel of the Project board (copied from the old QNC's
//! `qnc-project-blocks/settings.rs`): the title and subtitle, then in the order of the
//! layout contract (`pts_slots`) the template picker, New project, the Advanced bar and under
//! it the AI switches, the location fields with their browsers, the template actions, the
//! Advanced sections and the own template.

use eframe::egui::{self, RichText, Sense, Vec2};
use qnc_desktop_theme::Theme;
use qnc_project_layout::ProjectLayout;
use qnc_project_session::{ProjectIntent, ProjectSession};

use crate::{advanced, locations, picker};

/// One frame of the panel over the session.
pub(crate) struct Panel<'a> {
    pub layout: &'a ProjectLayout,
    pub theme: &'a Theme,
    pub session: &'a mut ProjectSession,
    pub root: &'a std::path::Path,
    pub intents: Vec<ProjectIntent>,
}

/// Draws the panel in `rect` and returns what the user asked. `root` is the QNC root the
/// language choice writes to.
pub fn show(ui: &mut egui::Ui, layout: &ProjectLayout, theme: &Theme, rect: egui::Rect, session: &mut ProjectSession, root: &std::path::Path) -> Vec<ProjectIntent> {
    let mut panel = Panel { layout, theme, session, root, intents: Vec::new() };
    let settings = &layout.right_settings_panel;
    ui.painter().rect_filled(rect, 0.0, theme.bg);
    let inner = rect.shrink(settings.panel_pad);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(inner);
        ui.set_min_width(settings.field_min_width);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        panel.head(ui, inner.width().max(40.0));
        panel.scroll(ui, inner.width().max(40.0), inner.bottom());
    });
    panel.intents
}

impl Panel<'_> {
    fn head(&mut self, ui: &mut egui::Ui, width: f32) {
        let settings = &self.layout.right_settings_panel;
        qnc_settings_rows::panel_title_row(ui, &settings.title, self.theme);
        egui::Frame::NONE.inner_margin(egui::Margin { left: settings.inner_pad_x as i8, right: settings.inner_pad_x as i8, top: 0, bottom: settings.inner_pad_y as i8 }).show(ui, |ui| {
            ui.set_max_width(width);
            ui.label(RichText::new(&settings.subtitle).size(12.0).color(self.theme.text_muted));
        });
    }

    fn scroll(&mut self, ui: &mut egui::Ui, width: f32, panel_bottom: f32) {
        let settings = self.layout.right_settings_panel.clone();
        let scroll_top = ui.cursor().top() + 1.0;
        let scroll_h = (panel_bottom - scroll_top).max(48.0);
        let scroll_rect = egui::Rect::from_min_size(egui::pos2(ui.min_rect().left(), scroll_top), Vec2::new(width, scroll_h));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(scroll_rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
            ui.set_clip_rect(scroll_rect);
            egui::ScrollArea::vertical().id_salt("pts_scroll").max_height(scroll_h).auto_shrink([false, true]).show(ui, |ui| {
                let pad = egui::Margin { left: settings.inner_pad_x as i8, right: settings.inner_pad_x as i8, top: settings.inner_pad_y as i8, bottom: settings.inner_pad_y as i8 };
                egui::Frame::NONE.inner_margin(pad).show(ui, |ui| {
                    // The content stays inside the margins.
                    let width = (width - 2.0 * settings.inner_pad_x).max(40.0);
                    ui.set_max_width(width);
                    ui.spacing_mut().item_spacing.y = settings.section_gap;
                    for slot in self.layout.pts_slots.fixed_order.clone() {
                        match slot.as_str() {
                            "TemplatePicker" => picker::template_picker(self, ui, width),
                            "ProjectCreate" => picker::project_create(self, ui, width),
                            _ => {}
                        }
                    }
                    // The Advanced bar; the slots of the scroll show only while it is open.
                    advanced::header(ui, width, &self.layout.advanced_settings, self.theme, &mut self.session.advanced_open);
                    let slots = if self.session.advanced_open { self.layout.pts_slots.scroll_order.clone() } else { Vec::new() };
                    for slot in slots {
                        match slot.as_str() {
                            "AiSettings" => self.ai_settings(ui, width),
                            "Locations" => locations::locations(self, ui, width),
                            "TemplateActions" => picker::template_actions(self, ui, width),
                            "AdvancedSections" => advanced::sections(self, ui, width),
                            "CustomTemplate" if self.session.template_create_open => picker::custom_template(self, ui, width),
                            _ => {}
                        }
                    }
                });
            });
        });
    }

    fn ai_settings(&mut self, ui: &mut egui::Ui, content_w: f32) {
        let metrics = &self.layout.right_settings_panel;
        let switches: Vec<qnc_settings_switches::Switch> = self.layout.ai_switches.iter().map(|switch| qnc_settings_switches::Switch { path: switch.path.clone(), on: switch.on.clone(), off: switch.off.clone(), default_on: switch.default_on, label: switch.label.clone() }).collect();
        qnc_settings_rows::section(ui, content_w, qnc_text::text("project.ai"), metrics, self.theme, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            if qnc_settings_switches::show(ui, &mut self.session.draft_settings, &switches) {
                self.session.status = self.layout.texts.settings_changed.clone();
            }
        });
    }
}

/// A clickable bar with a title and an open/closed arrow (the picker head, the Advanced bar).
pub(crate) fn bar(ui: &mut egui::Ui, width: f32, fill: egui::Color32, theme: &Theme, title: RichText, open: bool) -> bool {
    let head = egui::Frame::NONE.fill(fill).stroke(egui::Stroke::new(1.0, theme.border)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.set_max_width(width);
        ui.horizontal(|ui| {
            ui.label(title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(if open { "▲" } else { "▼" }).size(11.0).color(theme.text_muted));
            });
        });
    });
    head.response.interact(Sense::click()).clicked()
}
