use crate::{
    capabilities::{
        clipboard::ClipboardReader,
        paste::{self, PasteCompletion, PasteRequest},
    },
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

pub fn paste(model: &mut Model) -> Option<Effect> {
    let clipboard = model.register.provider();
    paste_with_reader(model, clipboard)
}

pub fn paste_with_reader<C>(model: &mut Model, clipboard: C) -> Option<Effect>
where
    C: ClipboardReader + Send + 'static,
{
    let request = paste::paste_with_reader(&mut model.provider_login, clipboard)?;
    Some(Effect::feature(ProviderLoginPasteRequest(request)))
}

struct ProviderLoginPasteRequest<C>(PasteRequest<C, u64>);

impl<C> FeatureEffect for ProviderLoginPasteRequest<C>
where
    C: ClipboardReader + Send + 'static,
{
    fn perform(self: Box<Self>) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(ProviderLoginPasteCompletion(
            self.0.perform(),
        )))
    }
}

struct ProviderLoginPasteCompletion(PasteCompletion<u64>);

impl Completion for ProviderLoginPasteCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match self.0.apply(&mut model.provider_login) {
            Some(Ok(())) => None,
            Some(Err(error)) => {
                model.provider_login.set_error(error.to_string());
                Some(crate::notifications::publish_status(
                    model,
                    StatusMessage::error(format!("Paste failed: {error}")),
                    Some("Provider"),
                ))
            }
            None => None,
        }
    }
}
