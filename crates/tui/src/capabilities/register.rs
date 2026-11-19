use helix_view::clipboard::ClipboardProvider;

use super::clipboard::ClipboardWriter;

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

/// One automatic unnamed register plus the detected system clipboard provider.
#[derive(Debug, Default)]
pub struct RegisterState {
    unnamed: String,
    provider: ClipboardProvider,
}

impl RegisterState {
    pub fn unnamed(&self) -> &str {
        &self.unnamed
    }

    pub fn has_system_provider(&self) -> bool {
        let ClipboardProvider::None = self.provider else {
            return true;
        };
        false
    }

    pub(crate) fn provider(&self) -> ClipboardProvider {
        self.provider.clone()
    }

    /// Replace the local register after a background clipboard read without writing it back.
    pub fn update_from_clipboard(&mut self, text: String) {
        self.unnamed = text;
    }

    #[cfg(test)]
    pub fn set_provider(&mut self, provider: ClipboardProvider) {
        self.provider = provider;
    }

    pub(crate) fn clipboard_write(&self, text: String) -> ClipboardWrite {
        ClipboardWrite {
            text,
            clipboard: self.provider.clone(),
        }
    }

    /// Update the local register immediately and mirror it in a blocking effect.
    pub fn record(&mut self, text: String) -> Option<Effect> {
        self.unnamed = text.clone();
        Some(Effect::feature(self.clipboard_write(text)))
    }
}

pub(crate) struct ClipboardWrite<C = ClipboardProvider> {
    text: String,
    clipboard: C,
}

impl<C: ClipboardWriter> ClipboardWrite<C> {
    pub(crate) fn execute(self) -> Result<(), String> {
        self.clipboard.write_text(&self.text)
    }
}

impl FeatureEffect for ClipboardWrite {
    fn perform(self: Box<Self>) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(ClipboardCompletion(
            ClipboardWrite::execute(*self),
        )))
    }
}

struct ClipboardCompletion(Result<(), String>);

impl Completion for ClipboardCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if let Err(error) = self.0 {
            return Some(crate::notifications::report_status(
                model,
                StatusMessage::error(format!("System clipboard unavailable: {error}")),
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_register_succeeds_even_when_clipboard_is_unavailable() {
        let mut register = RegisterState::default();
        let effect = register.record("hello".to_string());
        assert_eq!(register.unnamed(), "hello");
        assert!(effect.is_some());

        let write = ClipboardWrite {
            text: "hello".to_string(),
            clipboard: ClipboardProvider::None,
        };
        let Message::FeatureCompletion(completion) = Box::new(write).perform() else {
            panic!("expected clipboard completion");
        };
        let mut model = Model::default();
        completion.update(&mut model);
        assert!(
            model
                .status
                .as_ref()
                .unwrap()
                .text
                .contains("System clipboard unavailable")
        );
        assert_eq!(
            model.notifications.current().unwrap().kind(),
            crate::capabilities::notification::NotificationKind::Error
        );
    }
}
