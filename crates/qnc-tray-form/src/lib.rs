//! The tray form: the icon in the tray with its status and Quit, and the ingest window that
//! shows the cards being ingested. The run is polled every frame (it looks every half second);
//! the user's choices go to the run. The window shows itself while a card is ingested and
//! hides when the user says so; Quit in the tray menu ends everything.

use qnc_ingest_panel::{CardLine, Seen};
use qnc_tray_icon::{MenuItem, Tray, TrayEvent};
use qnc_tray_run::{Run, Told};
use qnc_window_host::Next;

const STATUS: u32 = 1;
const QUIT: u32 = 2;
const PROJECTS: u32 = 3;
const SETTINGS: u32 = 4;

fn menu(status: &str) -> Vec<MenuItem> {
    vec![
        MenuItem { id: STATUS, text: status.to_string(), enabled: false, checked: false },
        MenuItem { id: PROJECTS, text: qnc_text::text("tray.projects").to_string(), enabled: true, checked: false },
        MenuItem { id: SETTINGS, text: qnc_text::text("tray.settings").to_string(), enabled: true, checked: false },
        MenuItem { id: QUIT, text: qnc_text::text("tray.quit").to_string(), enabled: true, checked: false },
    ]
}

/// Runs the tray until the user quits.
pub fn main() -> Result<(), String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-tray");
    qnc_diagnostics::log_panics();
    qnc_settings::choose_language(&root)?;
    let mut run = Run::start(&root)?;
    let waiting = qnc_text::text("tray.waiting").to_string();
    let title = qnc_text::text("tray.title").to_string();
    let tray = Tray::start(&format!("{title}: {waiting}"), menu(&waiting))?;
    qnc_diagnostics::log("tray started");
    let mut shown = true;
    let mut quit = false;
    let mut cards: Vec<CardLine> = Vec::new();
    let outcome = qnc_window_host::run(&qnc_text::text("ingest.window_title"), [520.0, 360.0], true, move |_ctx, ui| {
        for told in run.poll() {
            match told {
                Told::Status(status) => {
                    tray.set_tip(&format!("{title}: {status}"));
                    tray.set_menu(menu(&status));
                }
                Told::Notice { title, text } => tray.notify(&title, &text),
                Told::Cards(lines) => {
                    shown = shown || (!lines.is_empty() && cards.is_empty());
                    cards = lines.into_iter().map(|c| CardLine { serial: c.serial, name: c.name, project: c.project, clips: c.clips, finished: c.finished, failed: c.failed, bytes_copied: c.bytes_copied }).collect();
                }
            }
        }
        tray.pump();
        while let Some(TrayEvent::Menu(id)) = tray.try_event() {
            match id {
                QUIT => quit = true,
                PROJECTS => run.open("qnc-projects"),
                SETTINGS => run.open("qnc-settings"),
                _ => {}
            }
        }
        for seen in qnc_ingest_panel::show(ui, qnc_theme::theme(), &cards, &waiting) {
            match seen {
                Seen::Stop(serial) => run.stop(&serial),
                Seen::Hide => shown = false,
            }
        }
        match (quit, shown) {
            (true, _) => {
                run.stop_all();
                qnc_diagnostics::log("tray quit");
                Next::Close
            }
            (false, true) => Next::Show,
            (false, false) => Next::Hide,
        }
    });
    if let Err(error) = &outcome {
        qnc_diagnostics::log(format!("window host ended with an error: {error}"));
    }
    outcome
}
