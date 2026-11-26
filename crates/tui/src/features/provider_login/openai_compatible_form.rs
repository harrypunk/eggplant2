use std::fmt;

use crate::component::{
    form::FormFocus,
    text_input::{TextInput, TextInputEvent},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiField {
    Endpoint,
    ApiKey,
}

/// Provider-neutral OpenAI-compatible endpoint and API-key draft.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenAiForm {
    endpoint: TextInput,
    api_key: TextInput,
    focus: FormFocus,
    revision: u64,
}

impl fmt::Debug for OpenAiForm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiForm")
            .field("endpoint", &self.endpoint.value())
            .field("focused", &self.focused_field())
            .field(
                "api_key",
                &(!self.api_key.value().is_empty()).then_some("[redacted]"),
            )
            .field("revision", &self.revision)
            .finish()
    }
}

impl Default for OpenAiForm {
    fn default() -> Self {
        Self::new("")
    }
}

impl OpenAiForm {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: TextInput::new(endpoint),
            api_key: TextInput::default(),
            focus: FormFocus::new(2),
            revision: 0,
        }
    }

    pub fn reset(&mut self) {
        self.reset_to("");
    }

    pub fn reset_to(&mut self, endpoint: &str) {
        self.endpoint.set_value(endpoint);
        self.api_key.clear();
        self.focus.reset();
        self.invalidate();
    }

    pub fn focused_field(&self) -> OpenAiField {
        match self.focus.index() {
            0 => OpenAiField::Endpoint,
            _ => OpenAiField::ApiKey,
        }
    }

    pub fn input(&self, field: OpenAiField) -> &TextInput {
        match field {
            OpenAiField::Endpoint => &self.endpoint,
            OpenAiField::ApiKey => &self.api_key,
        }
    }

    fn focused_input(&mut self) -> &mut TextInput {
        match self.focused_field() {
            OpenAiField::Endpoint => &mut self.endpoint,
            OpenAiField::ApiKey => &mut self.api_key,
        }
    }

    pub fn focus_next(&mut self) {
        self.focus.next();
        self.invalidate();
    }

    pub fn handle_input(&mut self, event: TextInputEvent) -> bool {
        let change = self.focused_input().handle_event(event);
        self.invalidate();
        change.content_changed()
    }

    pub fn values(&self) -> (&str, &str) {
        (self.endpoint.value(), self.api_key.value())
    }

    pub fn begin_paste(&mut self) -> u64 {
        self.invalidate();
        self.revision
    }

    /// Apply paste only while the form is at the same focus/edit revision.
    pub fn complete_paste(
        &mut self,
        revision: u64,
        text: Result<String, String>,
    ) -> Option<Result<(), String>> {
        if revision != self.revision {
            return None;
        }
        let text = match text {
            Ok(text) => text,
            Err(error) => return Some(Err(error)),
        };
        let text = text.trim();
        if text.is_empty() {
            return Some(Err("Clipboard is empty".to_string()));
        }
        if text.contains(['\n', '\r']) {
            return Some(Err("Pasted text must be single-line".to_string()));
        }
        self.focused_input().insert_str(text);
        self.invalidate();
        Some(Ok(()))
    }

    pub fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_login::DEFAULT_OPENAI_ENDPOINT;

    #[test]
    fn focus_navigation_and_editing_are_field_local() {
        let mut form = OpenAiForm::new(DEFAULT_OPENAI_ENDPOINT);
        assert_eq!(
            form.input(OpenAiField::Endpoint).value(),
            DEFAULT_OPENAI_ENDPOINT
        );
        form.focus_next();
        for character in "key-secret".chars() {
            form.handle_input(TextInputEvent::Insert(character));
        }
        assert_eq!(form.input(OpenAiField::ApiKey).value(), "key-secret");
        assert_eq!(
            form.input(OpenAiField::Endpoint).value(),
            DEFAULT_OPENAI_ENDPOINT
        );
        form.focus_next();
        assert_eq!(form.focused_field(), OpenAiField::Endpoint);
        form.focus_next();
        assert_eq!(form.focused_field(), OpenAiField::ApiKey);
        assert!(!format!("{form:?}").contains("key-secret"));
    }

    #[test]
    fn paste_is_trimmed_and_stale_revisions_are_ignored() {
        let mut form = OpenAiForm::new(DEFAULT_OPENAI_ENDPOINT);
        form.endpoint.clear();
        let revision = form.begin_paste();
        assert!(
            form.complete_paste(revision, Ok("https://localhost:1234/v1\n".to_string()))
                .unwrap()
                .is_ok()
        );
        assert_eq!(
            form.input(OpenAiField::Endpoint).value(),
            "https://localhost:1234/v1"
        );

        form.focus_next();
        let stale = form.begin_paste();
        form.focus_next();
        assert!(
            form.complete_paste(stale, Ok("ignored".to_string()))
                .is_none()
        );
        assert!(form.input(OpenAiField::ApiKey).value().is_empty());

        form.focus_next();
        let invalid = form.begin_paste();
        assert!(
            form.complete_paste(invalid, Ok("two\nlines".to_string()))
                .unwrap()
                .is_err()
        );
        assert!(form.input(OpenAiField::ApiKey).value().is_empty());
    }
}
