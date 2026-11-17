use super::clipboard::ClipboardWriter;

const SHORT_HASH_LEN: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashFormat {
    Short,
    Full,
}

/// An owned hash-copy operation with its output capability injected.
pub struct CopyHash<C> {
    hash: String,
    format: HashFormat,
    clipboard: C,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyHashOutcome {
    pub format: HashFormat,
    pub text: String,
    pub result: Result<(), String>,
}

impl<C: ClipboardWriter> CopyHash<C> {
    pub fn new(hash: String, format: HashFormat, clipboard: C) -> Self {
        Self {
            hash,
            format,
            clipboard,
        }
    }

    pub fn execute(self) -> CopyHashOutcome {
        let text = match self.format {
            HashFormat::Short => self.hash.chars().take(SHORT_HASH_LEN).collect(),
            HashFormat::Full => self.hash,
        };
        let result = self.clipboard.write_text(&text);
        CopyHashOutcome {
            format: self.format,
            text,
            result,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct MemoryClipboard {
        text: RefCell<String>,
        failure: Option<String>,
    }

    impl ClipboardWriter for MemoryClipboard {
        fn write_text(&self, text: &str) -> Result<(), String> {
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            *self.text.borrow_mut() = text.to_string();
            Ok(())
        }
    }

    #[test]
    fn copies_short_or_full_hash_through_the_injected_port() {
        let full = "0123456789abcdef0123456789abcdef01234567".to_string();
        let clipboard = MemoryClipboard::default();
        let outcome = CopyHash::new(full.clone(), HashFormat::Short, &clipboard).execute();
        assert_eq!(outcome.text, "0123456");
        assert_eq!(outcome.result, Ok(()));
        assert_eq!(clipboard.text.borrow().as_str(), "0123456");

        let outcome = CopyHash::new(full.clone(), HashFormat::Full, &clipboard).execute();
        assert_eq!(outcome.text, full);
        assert_eq!(outcome.result, Ok(()));
        assert_eq!(clipboard.text.borrow().as_str(), full);
    }

    #[test]
    fn returns_clipboard_failure_without_losing_copy_identity() {
        let clipboard = MemoryClipboard {
            failure: Some("unavailable".to_string()),
            ..MemoryClipboard::default()
        };
        let outcome =
            CopyHash::new("abc123456789".to_string(), HashFormat::Short, clipboard).execute();
        assert_eq!(outcome.format, HashFormat::Short);
        assert_eq!(outcome.text, "abc1234");
        assert_eq!(outcome.result, Err("unavailable".to_string()));
    }
}
