#[cfg(not(test))]
use std::{fmt, fs::OpenOptions, io::Write};

/// Keep tests independent of the real user log file.
#[cfg(test)]
pub fn log(_message: std::fmt::Arguments) {}

/// Append a diagnostic line without writing to the terminal or exposing failures to the UI loop.
#[cfg(not(test))]
pub fn log(message: fmt::Arguments) {
    let Some(home) = std::env::home_dir() else {
        return;
    };
    let directory = home.join(".eggplant").join("log");
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("app.log"))
    else {
        return;
    };
    let _ = writeln!(file, "{} {}", chrono::Utc::now().to_rfc3339(), message);
}
