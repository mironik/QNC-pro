//! The tray form: the icon in the tray with the menu the run gives (its status line, Stop all the
//! work, the modes with their tick boxes, Projects, Settings, Open the application, Quit) and a
//! small window shown only while the run asks what becomes of the work before it stops (Keep,
//! Delete, Cancel). The run is polled every frame; the user's choices and answers go to the run.

use eframe::egui::{self, RichText};
use qnc_tray_icon::{MenuItem, Tray, TrayEvent};
use qnc_tray_run::{Item, Run, StopAnswer, StopAsk, Told};
use qnc_window_host::Next;

/// The menu items by the id the icon gives them back (their place, from one).
fn menu(items: &[Item]) -> Vec<MenuItem> {
    items.iter().enumerate().map(|(at, item)| MenuItem { id: at as u32 + 1, text: item.text.clone(), enabled: item.enabled, check: item.check }).collect()
}

/// Runs the tray until the user quits.
pub fn main() -> Result<(), String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-tray");
    qnc_diagnostics::log_panics();
    qnc_settings::choose_language(&root)?;
    let mut run = Run::start(&root)?;
    let title = qnc_text::text("tray.title").to_string();
    let tray = Tray::start(&format!("{title}: {}", qnc_text::text("tray.waiting")), Vec::new(), Some(qnc_app_mark::qnc_mark))?;
    qnc_diagnostics::log("tray started");
    let mut items: Vec<Item> = Vec::new();
    let mut ask: Option<StopAsk> = None;
    let mut quit = false;
    let window_title = title.clone();
    let outcome = qnc_window_host::run(&window_title, [560.0, 220.0], false, move |_ctx, ui| {
        let mut told = run.poll();
        tray.pump();
        while let Some(TrayEvent::Menu(id)) = tray.try_event() {
            if let Some(chosen) = items.get(id as usize - 1).and_then(|item| item.choice) {
                told.extend(run.choose(chosen));
            }
        }
        if let Some(asked) = &ask {
            if let Some(answer) = question(ui, asked) {
                told.extend(run.answer_stop(answer));
            }
        }
        for t in told {
            match t {
                Told::Status(status) => tray.set_tip(&format!("{title}: {status}")),
                Told::Menu(given) => {
                    tray.set_menu(menu(&given));
                    items = given;
                }
                Told::Notice { title, text } => tray.notify(&title, &text),
                Told::Ask(asked) => ask = asked,
                Told::Quit => quit = true,
            }
        }
        match (quit, ask.is_some()) {
            (true, _) => Next::Close,
            (false, true) => Next::Show,
            (false, false) => Next::Hide,
        }
    });
    if let Err(error) = &outcome {
        qnc_diagnostics::log(format!("window host ended with an error: {error}"));
    }
    outcome
}

/// The question before the work stops, with Keep, Delete and Cancel.
fn question(ui: &mut egui::Ui, asked: &StopAsk) -> Option<StopAnswer> {
    let theme = qnc_theme::theme();
    let text = qnc_ui_kit::colour(theme.colours.text);
    ui.add_space(8.0);
    ui.label(RichText::new(&asked.text).color(text));
    ui.add_space(16.0);
    let mut answer = None;
    ui.horizontal(|ui| {
        if qnc_ui_kit::button(ui, theme, qnc_text::text("tray.keep"), true, false).clicked() {
            answer = Some(StopAnswer::Keep);
        }
        if qnc_ui_kit::button(ui, theme, qnc_text::text("tray.delete"), true, false).clicked() {
            answer = Some(StopAnswer::Delete);
        }
        if qnc_ui_kit::button(ui, theme, qnc_text::text("tray.cancel"), true, true).clicked() {
            answer = Some(StopAnswer::Cancel);
        }
    });
    answer
}
