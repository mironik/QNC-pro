//! The tray form: the icon in the tray, a menu of the status line and Quit. The run gives the
//! status and notices each tick; Quit stops the ingests and ends.

use qnc_tray_icon::{MenuItem, Tray, TrayEvent};
use qnc_tray_run::{Run, Told};

const STATUS: u32 = 1;
const QUIT: u32 = 2;

fn menu(status: &str) -> Vec<MenuItem> {
    vec![
        MenuItem { id: STATUS, text: status.to_string(), enabled: false, checked: false },
        MenuItem { id: QUIT, text: qnc_text::text("tray.quit").to_string(), enabled: true, checked: false },
    ]
}

/// Runs the tray until the user quits.
pub fn main() -> Result<(), String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-tray");
    qnc_diagnostics::log_panics();
    let mut run = Run::start(&root)?;
    let waiting = qnc_text::text("tray.waiting").to_string();
    let tray = Tray::start(&format!("{}: {waiting}", qnc_text::text("tray.title")), menu(&waiting))?;
    qnc_diagnostics::log("tray started");
    loop {
        for told in run.tick() {
            match told {
                Told::Status(status) => {
                    tray.set_tip(&format!("{}: {status}", qnc_text::text("tray.title")));
                    tray.set_menu(menu(&status));
                }
                Told::Notice { title, text } => tray.notify(&title, &text),
            }
        }
        tray.pump();
        while let Some(TrayEvent::Menu(id)) = tray.try_event() {
            if id == QUIT {
                run.stop_all();
                qnc_diagnostics::log("tray quit");
                return Ok(());
            }
        }
    }
}
