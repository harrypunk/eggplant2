use helix_view::clipboard::{ClipboardProvider, ClipboardType};

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
        !matches!(self.provider, ClipboardProvider::None)
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

    /// Update the local register immediately and mirror it in a blocking effect.
    pub fn record(&mut self, text: String) -> Option<Effect> {
        self.unnamed = text.clone();
        Some(Effect::feature(ClipboardWrite {
            text,
            provider: self.provider.clone(),
        }))
    }
}

struct ClipboardWrite {
    text: String,
    provider: ClipboardProvider,
}

impl FeatureEffect for ClipboardWrite {
    fn perform(self: Box<Self>) -> Message {
        let result = if matches!(&self.provider, ClipboardProvider::None) {
            Err("no system clipboard provider is available".to_string())
        } else {
            self.provider
                .set_contents(&self.text, ClipboardType::Clipboard)
                .map_err(|error| error.to_string())
        };
        Message::FeatureCompletion(FeatureCompletion::new(ClipboardCompletion(result)))
    }
}

struct ClipboardCompletion(Result<(), String>);

impl Completion for ClipboardCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if let Err(error) = self.0 {
            model.status = Some(StatusMessage::error(format!(
                "System clipboard unavailable: {error}"
            )));
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
            provider: ClipboardProvider::None,
        };
        let Message::FeatureCompletion(completion) = Box::new(write).perform() else {
            panic!("expected clipboard completion");
        };
        let mut model = Model::default();
        completion.update(&mut model);
        assert!(
            model
                .status
                .unwrap()
                .text
                .contains("System clipboard unavailable")
        );
    }
}
