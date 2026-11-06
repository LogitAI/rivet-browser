//! Helper processes Rivet starts on its own, such as `rivet hold refresh`, `rivet session start`
//! from the launcher, or `icacls`. They are console programs, so Windows gives each one a console
//! window unless it is told not to. A person opening Rivet must never see those windows.

use std::process::Command;

/// `CREATE_NO_WINDOW`: the child gets no console window of its own.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The process creation flags for a helper started in the background.
pub fn creation_flags() -> u32 {
    CREATE_NO_WINDOW
}

/// Makes `command` start without a console window.
pub fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(creation_flags());
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr5_helper_processes_start_without_a_console_window() {
        assert_eq!(creation_flags() & CREATE_NO_WINDOW, CREATE_NO_WINDOW);
        let mut command = Command::new("rivet");
        hidden(&mut command);
        assert_eq!(command.get_program(), "rivet");
    }
}
