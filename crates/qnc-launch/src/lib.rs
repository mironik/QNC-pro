//! Starting helper programs. A program is named by its name and found beside the application
//! (the same folder, or the one above it in a build tree); never one from the PATH, so the
//! application runs with the helpers it was shipped with. A helper runs without a console
//! window and, when asked, below the application's priority, so a worker never takes the
//! processor from the player or the interface.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// As the application (a player, a plug-in the user waits for).
    Same,
    /// Below the application (a background worker).
    Below,
}

/// The file of the program `name` beside the application.
pub fn beside_application(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err(qnc_text::fill("launch.not_a_name", &[("name", name)]));
    }
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.ancestors()
        .skip(1)
        .take(2)
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
        .ok_or_else(|| qnc_text::fill("launch.not_beside", &[("name", name)]))
}

/// A command for a program file: no console window, the given priority.
pub fn command(program: &Path, priority: Priority) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        let flags = match priority {
            Priority::Same => CREATE_NO_WINDOW,
            Priority::Below => CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS,
        };
        command.creation_flags(flags);
    }
    #[cfg(not(windows))]
    {
        let _ = priority;
    }
    command
}

/// Starts a program beside the application with piped stdin and stdout (a plug-in talks over
/// them); stderr is inherited.
pub fn start(name: &str, args: &[&str], priority: Priority) -> Result<Child, String> {
    let program = beside_application(name)?;
    command(&program, priority)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| qnc_text::fill("launch.failed", &[("name", name), ("error", &e.to_string())]))
}

/// Starts a program beside the application detached: nothing piped, it outlives the caller.
pub fn start_detached(name: &str, args: &[&str], priority: Priority) -> Result<Child, String> {
    let program = beside_application(name)?;
    command(&program, priority)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| qnc_text::fill("launch.failed", &[("name", name), ("error", &e.to_string())]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_named_by_name_and_must_be_beside_the_application() {
        assert!(beside_application("a/b").is_err());
        assert!(beside_application("").is_err());
        assert!(beside_application("qnc-no-such-program").unwrap_err().contains("qnc-no-such-program"));
        assert!(start("qnc-no-such-program", &[], Priority::Below).is_err());
    }
}
