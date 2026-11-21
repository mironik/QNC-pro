//! The native window every QNC form lives in. The host opens it (hidden when asked), applies the
//! theme, and calls the form's `frame` on every repaint and at least every half second; the form
//! draws into the central panel and answers with what the window should do next.

use std::time::Duration;

use egui::{Context, Ui};

pub const PULSE: Duration = Duration::from_millis(500);

/// What the form wants of the window after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Keep going, shown.
    Show,
    /// Keep going, hidden (the icon in the tray stays).
    Hide,
    /// End the program.
    Close,
}

struct Host<F: FnMut(&Context, &mut Ui) -> Next> {
    frame: F,
    visible: bool,
}

impl<F: FnMut(&Context, &mut Ui) -> Next> eframe::App for Host<F> {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let mut next = Next::Show;
        egui::CentralPanel::default().show(ctx, |ui| {
            next = (self.frame)(ctx, ui);
        });
        match next {
            Next::Show | Next::Hide => {
                let wanted = next == Next::Show;
                if wanted != self.visible {
                    self.visible = wanted;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(wanted));
                }
                ctx.request_repaint_after(PULSE);
            }
            Next::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

/// Runs the window until the form closes it. `visible` says whether it opens shown.
pub fn run(title: &str, size: [f32; 2], visible: bool, frame: impl FnMut(&Context, &mut Ui) -> Next + 'static) -> Result<(), String> {
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title(title).with_inner_size(size).with_visible(visible), ..Default::default() };
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            qnc_ui_kit::apply(&cc.egui_ctx, qnc_theme::theme());
            Ok(Box::new(Host { frame, visible }))
        }),
    )
    .map_err(|e| e.to_string())
}

/// Runs a window with QNC's own title bar (no system frame) and the margin under the form; the
/// form draws into the central panel between them. `size` is the content's, the bar is added.
pub fn run_framed(title: &str, size: [f32; 2], visible: bool, frame: impl FnMut(&Context, &mut Ui) -> Next + 'static) -> Result<(), String> {
    let viewport = egui::ViewportBuilder::default().with_title(title).with_inner_size([size[0], size[1] + qnc_title_bar::HEIGHT]).with_visible(visible);
    let options = eframe::NativeOptions { viewport: qnc_title_bar::frameless(viewport), ..Default::default() };
    let bar = qnc_title_bar::TitleBar::new(title);
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            qnc_ui_kit::apply(&cc.egui_ctx, qnc_theme::theme());
            Ok(Box::new(Framed { bar, host: Host { frame, visible } }))
        }),
    )
    .map_err(|e| e.to_string())
}

struct Framed<F: FnMut(&Context, &mut Ui) -> Next> {
    bar: qnc_title_bar::TitleBar,
    host: Host<F>,
}

impl<F: FnMut(&Context, &mut Ui) -> Next> eframe::App for Framed<F> {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.bar.show(ctx);
        qnc_title_bar::bottom_margin(ctx);
        self.host.update(ctx, frame);
    }
}
