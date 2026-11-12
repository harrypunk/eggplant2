use std::{io::stdout, panic};

use ratatui::{
    DefaultTerminal, Terminal,
    backend::CrosstermBackend,
    crossterm::{
        ExecutableCommand,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};

pub fn init_terminal() -> color_eyre::Result<DefaultTerminal> {
    enable_raw_mode()?;
    if let Err(error) = stdout().execute(EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }
    match Terminal::new(CrosstermBackend::new(stdout())) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let _ = restore_terminal();
            Err(error.into())
        }
    }
}

pub fn restore_terminal() -> color_eyre::Result<()> {
    // Always attempt both operations, even if leaving the alternate screen fails.
    let leave_result = stdout().execute(LeaveAlternateScreen).map(|_| ());
    let raw_mode_result = disable_raw_mode();
    leave_result?;
    raw_mode_result?;
    Ok(())
}

pub fn install_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Cleanup must never trigger a second panic while already unwinding.
        let _ = restore_terminal();
        original_hook(panic_info);
    }));
}
