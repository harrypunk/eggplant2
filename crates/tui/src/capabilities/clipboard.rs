use helix_view::clipboard::{ClipboardProvider, ClipboardType};

/// Minimal text-input port used by paste-oriented capabilities.
pub trait ClipboardReader {
    fn read_text(&self) -> Result<String, String>;
}

impl<C: ClipboardReader + ?Sized> ClipboardReader for &C {
    fn read_text(&self) -> Result<String, String> {
        (*self).read_text()
    }
}

impl ClipboardReader for ClipboardProvider {
    fn read_text(&self) -> Result<String, String> {
        if let ClipboardProvider::None = self {
            return Err("no system clipboard provider is available".to_string());
        }
        self.get_contents(&ClipboardType::Clipboard)
            .map_err(|error| error.to_string())
    }
}

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
        if let ClipboardProvider::None = self {
            return Err("no system clipboard provider is available".to_string());
        }
        self.set_contents(text, ClipboardType::Clipboard)
            .map_err(|error| error.to_string())
    }
}
