use crate::{
    capabilities::clipboard::ClipboardReader,
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

pub fn paste(model: &mut Model) -> Option<Effect> {
    paste_with_reader(model, model.register.provider())
}

pub fn paste_with_reader<C>(model: &mut Model, clipboard: C) -> Option<Effect>
where
    C: ClipboardReader + Send + 'static,
{
    let revision = model.provider_login.begin_openai_paste()?;
    Some(Effect::feature(PasteRequest {
        clipboard,
        revision,
    }))
}

struct PasteRequest<C> {
    clipboard: C,
    revision: u64,
}

impl<C> FeatureEffect for PasteRequest<C>
where
    C: ClipboardReader + Send + 'static,
{
    fn perform(self: Box<Self>) -> Message {
        let result = self.clipboard.read_text();
        Message::FeatureCompletion(FeatureCompletion::new(PasteCompletion {
            revision: self.revision,
            result,
        }))
    }
}

struct PasteCompletion {
    revision: u64,
    result: Result<String, String>,
}

impl Completion for PasteCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match model
            .provider_login
            .complete_openai_paste(self.revision, self.result)
        {
            Some(Ok(())) => None,
            Some(Err(error)) => {
                model.provider_login.set_error(error.clone());
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
