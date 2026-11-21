//! The editor of a form's look (copied from the old QNC's `qnc-form-look`, its drawing part;
//! the data is `qnc-form-look-model`). Editing is on the form itself: while this computer's
//! setting opens the editor, every part the form marks (a margin, a gap, a title, a column
//! edge) shows a faint frame under the mouse; a click takes it (red), dragging it or the arrows
//! move it (Shift: ten steps), Esc lets it go. The form reads the new value in the same frame.
//! A small window beside the form has Save (writes the look file), Back (returns to what was
//! saved), the taken part's value and all values as numbers.

use std::{cell::RefCell, path::Path};

use eframe::egui;
pub use qnc_form_look_model::{editor_on, editor_setting, look_file, Value};

/// Which way a marked part grows: dragging or the arrow that way makes its value bigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grows {
    Right,
    Left,
    Down,
    Up,
}

impl Grows {
    fn along(self, delta: egui::Vec2) -> f32 {
        match self {
            Grows::Right => delta.x,
            Grows::Left => -delta.x,
            Grows::Down => delta.y,
            Grows::Up => -delta.y,
        }
    }
}

/// The look of one form with its editor.
pub struct FormLook {
    look: qnc_form_look_model::FormLook,
    editing: bool,
    window: bool,
    taken: RefCell<Option<String>>,
    note: String,
    sized: std::cell::Cell<Option<egui::Vec2>>,
}

/// Asks the form of this window to open its look editor, or shut it when open (the hidden button
/// before the title). Read by [`FormLook::editor`] in the same or the next frame.
pub fn ask_editor(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("qnc-form-look-asked"), true));
}

fn take_asked(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| data.remove_temp::<bool>(egui::Id::new("qnc-form-look-asked")).unwrap_or(false))
}

impl FormLook {
    /// Reads the look of `form`; every key in `keys` must be there.
    pub fn load(root: &Path, form: &str, keys: &[&str]) -> Result<Self, String> {
        let look = qnc_form_look_model::FormLook::load(root, form, keys)?;
        let editing = editor_on(root);
        Ok(Self { look, editing, window: editing, taken: RefCell::new(None), note: String::new(), sized: std::cell::Cell::new(None) })
    }

    /// The value of `key` (checked present when read).
    pub fn get(&self, key: &str) -> f32 {
        self.look.get(key)
    }

    pub fn editing(&self) -> bool {
        self.editing
    }

    pub fn has(&self, key: &str) -> bool {
        self.look.has(key)
    }

    /// Every value as it is now, to see whether it moved since the form last took it.
    pub fn stamp(&self) -> Vec<f32> {
        self.look.stamp()
    }

    /// Every value that points into the form's layout contract, with its pointer.
    pub fn pointed(&self) -> Vec<(String, f32)> {
        self.look.pointed()
    }

    pub fn changed(&self) -> bool {
        self.look.changed()
    }

    /// The look as data.
    pub fn model(&self) -> &qnc_form_look_model::FormLook {
        &self.look
    }

    /// A part of the form drawn over `rect` whose size is `key`, growing `grows`. While editing it
    /// can be taken with a click and moved by dragging or with the arrows.
    pub fn mark(&self, ui: &egui::Ui, rect: egui::Rect, key: &str, grows: Grows) {
        if !self.editing || !rect.is_positive() {
            return;
        }
        // A thin part (a gap of a few pixels) is still easy to hit.
        let hit = rect.expand2(egui::vec2(if rect.width() < 6.0 { 3.0 } else { 0.0 }, if rect.height() < 6.0 { 3.0 } else { 0.0 }));
        let response = ui.interact(hit, ui.id().with(("qnc-form-look-mark", key, rect.min.x as i32, rect.min.y as i32)), egui::Sense::click_and_drag());
        let taken = self.taken.borrow().as_deref() == Some(key);
        if response.clicked() || response.drag_started() {
            *self.taken.borrow_mut() = Some(key.to_string());
        }
        if response.dragged() {
            self.look.change(key, grows.along(response.drag_delta()), false);
        }
        if response.drag_stopped() {
            self.look.change(key, 0.0, true);
        }
        if taken {
            let step = self.look.step_of(key) * if ui.input(|input| input.modifiers.shift) { 10.0 } else { 1.0 };
            let (more, less) = match grows {
                Grows::Right => (egui::Key::ArrowRight, egui::Key::ArrowLeft),
                Grows::Left => (egui::Key::ArrowLeft, egui::Key::ArrowRight),
                Grows::Down => (egui::Key::ArrowDown, egui::Key::ArrowUp),
                Grows::Up => (egui::Key::ArrowUp, egui::Key::ArrowDown),
            };
            let (up, down) = ui.input_mut(|input| (input.count_and_consume_key(egui::Modifiers::NONE, more) + input.count_and_consume_key(egui::Modifiers::SHIFT, more), input.count_and_consume_key(egui::Modifiers::NONE, less) + input.count_and_consume_key(egui::Modifiers::SHIFT, less)));
            if up + down > 0 {
                self.look.change(key, step * (up as f32 - down as f32), true);
            }
            if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                *self.taken.borrow_mut() = None;
            }
        }
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("qnc-form-look-marks")));
        let red = egui::Color32::from_rgb(230, 70, 60);
        if taken {
            painter.rect_filled(rect, 0.0, red.gamma_multiply(0.25));
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.5, red), egui::StrokeKind::Inside);
            painter.text(rect.left_top() + egui::vec2(2.0, -2.0), egui::Align2::LEFT_BOTTOM, self.look.shown(key), egui::FontId::proportional(12.0), red);
        } else if response.hovered() {
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, red.gamma_multiply(0.7)), egui::StrokeKind::Inside);
        }
    }

    /// The window's size as the look says (`window_width`, `window_height`), given to the window
    /// when it changed; `extra` is what the window adds above its content (a title bar).
    pub fn window(&self, ctx: &egui::Context, extra: f32) {
        let size = egui::vec2(self.get("window_width"), self.get("window_height"));
        if self.sized.get() != Some(size) {
            self.sized.set(Some(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size + egui::vec2(0.0, extra)));
        }
    }

    /// The form's body in a panel of colour `fill` with the look's four margins (`margin_left`,
    /// `margin_right`, `margin_top`, `margin_bottom`), each moved where it is.
    pub fn panel<R>(&self, ctx: &egui::Context, fill: egui::Color32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let (left, right, top, bottom) = (self.get("margin_left"), self.get("margin_right"), self.get("margin_top"), self.get("margin_bottom"));
        let margin = egui::Margin { left: left.round() as i8, right: right.round() as i8, top: top.round() as i8, bottom: bottom.round() as i8 };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(fill).inner_margin(margin))
            .show(ctx, |ui| {
                let inner = ui.max_rect();
                let (xs, ys) = (inner.left() - left..=inner.right() + right, inner.top() - top..=inner.bottom() + bottom);
                self.mark(ui, egui::Rect::from_x_y_ranges(inner.left() - left..=inner.left(), ys.clone()), "margin_left", Grows::Right);
                self.mark(ui, egui::Rect::from_x_y_ranges(inner.right()..=inner.right() + right, ys), "margin_right", Grows::Left);
                self.mark(ui, egui::Rect::from_x_y_ranges(xs.clone(), inner.top() - top..=inner.top()), "margin_top", Grows::Down);
                self.mark(ui, egui::Rect::from_x_y_ranges(xs, inner.bottom()..=inner.bottom() + bottom), "margin_bottom", Grows::Up);
                add(ui)
            })
            .inner
    }

    /// A gap of `key` (going down), marked.
    pub fn space(&self, ui: &mut egui::Ui, key: &str) {
        let top = ui.cursor().top();
        ui.add_space(self.get(key));
        let rect = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
        self.mark(ui, rect, key, Grows::Down);
    }

    /// Opens the editor (its window, and parts of the form taken by click) or shuts both.
    fn shut_or_open(&mut self, open: bool) {
        self.window = open;
        self.editing = open;
        if !open {
            *self.taken.borrow_mut() = None;
        }
    }

    /// The small window beside the form while editing: Save, Back, and every value as a number.
    /// Call once per frame.
    pub fn editor(&mut self, ctx: &egui::Context) {
        if take_asked(ctx) {
            let open = !self.window;
            self.shut_or_open(open);
        }
        if !self.window {
            return;
        }
        let id = egui::ViewportId::from_hash_of(("qnc-form-look", self.look.form()));
        let builder = egui::ViewportBuilder::default().with_title(qnc_text::fill("form_look.title", &[("form", self.look.form())])).with_inner_size([380.0, 640.0]);
        ctx.show_viewport_immediate(id, builder, |ctx, _| {
            if ctx.input(|input| input.viewport().close_requested()) {
                self.shut_or_open(false);
            }
            egui::TopBottomPanel::bottom("qnc-form-look-buttons").show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.changed(), egui::Button::new(qnc_text::text("form_look.save"))).clicked() {
                        // Save writes and shuts the editor; a failed save keeps it open and says why.
                        match self.look.save() {
                            Ok(()) => {
                                self.note.clear();
                                self.shut_or_open(false);
                            }
                            Err(error) => self.note = error,
                        }
                    }
                    if ui.add_enabled(self.changed(), egui::Button::new(qnc_text::text("form_look.back"))).clicked() {
                        self.look.back();
                    }
                });
                ui.label(qnc_text::text("form_look.hint"));
                if !self.note.is_empty() {
                    ui.label(&self.note);
                }
                ui.add_space(6.0);
            });
            egui::CentralPanel::default().show(ctx, |ui| {
                let taken = self.taken.borrow().clone();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("qnc-form-look-values").num_columns(2).spacing([12.0, 8.0]).striped(true).show(ui, |ui| {
                        let mut values = self.look.values().borrow_mut();
                        for (value, saved) in values.iter_mut().zip(self.look.saved()) {
                            let mut label = egui::RichText::new(&value.label);
                            if value.value != saved.value {
                                label = label.strong();
                            }
                            if taken.as_deref() == Some(value.key.as_str()) {
                                label = label.color(egui::Color32::from_rgb(230, 70, 60));
                            }
                            ui.label(label).on_hover_text(&value.key);
                            ui.add(egui::DragValue::new(&mut value.value).range(value.min..=value.max).speed(value.step as f64));
                            ui.end_row();
                        }
                    });
                });
            });
        });
    }
}

impl FormLook {
    /// The editor over a look already read (by `qnc-form-look-model`); whether it edits is this
    /// computer's setting.
    pub fn of(root: &Path, look: qnc_form_look_model::FormLook) -> Self {
        let editing = editor_on(root);
        Self { look, editing, window: editing, taken: RefCell::new(None), note: String::new(), sized: std::cell::Cell::new(None) }
    }
}
