use helix_view::clipboard::{ClipboardProvider, ClipboardType};

/// Minimal text-output port used by copy-oriented capabilities.
pub trait ClipboardWriter {
    fn write_text(&self, text: &str) -> Result<(), String>;
}

impl<C: ClipboardWriter + ?Sized> ClipboardWriter for &C {
    fn write_text(&self, text: &str) -> Result<(), String> {
        (*self).write_text(text)
    }
}

impl ClipboardWriter for ClipboardProvider {
    fn write_text(&self, text: &str) -> Result<(), String> {
        if matches!(self, ClipboardProvider::None) {
            return Err("no system clipboard provider is available".to_string());
        }
        self.set_contents(text, ClipboardType::Clipboard)
            .map_err(|error| error.to_string())
    }
}
