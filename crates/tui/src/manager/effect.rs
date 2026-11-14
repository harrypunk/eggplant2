use editor::{DocumentOpenRequest, SaveRequest};
use tokio::sync::mpsc::UnboundedSender;

use crate::data_object::{Message, Model};

/// Typed worker result routed back to its owning feature on the main thread.
pub trait Completion: Send + 'static {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect>;
}

pub struct FeatureCompletion(Box<dyn Completion>);

impl std::fmt::Debug for FeatureCompletion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FeatureCompletion(..)")
    }
}

impl FeatureCompletion {
    pub fn new(completion: impl Completion) -> Self {
        Self(Box::new(completion))
    }

    pub fn update(self, model: &mut Model) -> Option<Effect> {
        self.0.update(model)
    }
}

/// Feature-owned background work. New effects implement `perform` and inherit
/// blocking execution unless they require different scheduling semantics.
pub trait FeatureEffect: Send + 'static {
    fn perform(self: Box<Self>) -> Message;

    fn execute(self: Box<Self>, sender: UnboundedSender<Message>) {
        tokio::task::spawn_blocking(move || {
            let _ = sender.send(self.perform());
        });
    }
}

pub enum Effect {
    Save(SaveRequest),
    OpenDocument(DocumentOpenRequest),
    Feature(Box<dyn FeatureEffect>),
}

impl Effect {
    pub fn feature(request: impl FeatureEffect) -> Self {
        Self::Feature(Box::new(request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEffect;

    impl FeatureEffect for TestEffect {
        fn perform(self: Box<Self>) -> Message {
            Message::FocusLost
        }
    }

    #[tokio::test]
    async fn feature_effect_executes_and_returns_a_completion_message() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let effect = Effect::feature(TestEffect);
        let Effect::Feature(effect) = effect else {
            panic!("expected a feature effect");
        };

        effect.execute(sender);

        assert!(matches!(receiver.recv().await, Some(Message::FocusLost)));
    }
}
