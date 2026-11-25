use crate::data_object::{Context, Message};

/// Semantic input together with the context that resolved its physical key binding.
///
/// Keeping the origin beside the message prevents update-time dispatchers from
/// recomputing routing policy through the global model.
#[derive(Debug)]
pub struct RoutedInput {
    pub context: Context,
    pub message: Message,
}

impl RoutedInput {
    pub fn new(context: Context, message: Message) -> Self {
        Self { context, message }
    }
}
