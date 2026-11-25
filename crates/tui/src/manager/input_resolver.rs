use crate::{
    data_object::{Context, Message},
    manager::{
        context::{InputAction, KeyChord},
        keyboard_input_registry::Registry,
        text_input_keymap::TextInputResolver,
    },
};

/// Ordered, feature-agnostic input resolution boundary.
pub trait InputResolver: std::fmt::Debug + Send + Sync {
    fn resolve(&self, context: Context, key: KeyChord) -> Option<Message>;
}

#[derive(Debug, Default)]
pub struct InputResolverChain {
    resolvers: Vec<Box<dyn InputResolver>>,
}

impl InputResolverChain {
    pub fn new(resolvers: Vec<Box<dyn InputResolver>>) -> Self {
        Self { resolvers }
    }

    pub fn with(mut self, resolver: impl InputResolver + 'static) -> Self {
        self.resolvers.push(Box::new(resolver));
        self
    }
}

impl InputResolver for InputResolverChain {
    fn resolve(&self, context: Context, key: KeyChord) -> Option<Message> {
        self.resolvers
            .iter()
            .find_map(|resolver| resolver.resolve(context, key))
    }
}

/// Direct command/general-action bindings, first in the default pipeline.
#[derive(Debug)]
pub struct DirectInputResolver {
    registry: Registry,
}

impl DirectInputResolver {
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }
}

impl InputResolver for DirectInputResolver {
    fn resolve(&self, context: Context, key: KeyChord) -> Option<Message> {
        self.registry
            .get_binding(&context)
            .and_then(|bindings| bindings.resolve_chord(key))
            .map(|action| match action {
                InputAction::Command(id) => Message::InvokeCommand(id),
                InputAction::General(action) => Message::GeneralAction(action),
            })
    }
}

/// Compose the default direct-binding and text-input resolvers outside EventManager.
pub fn default_input_resolvers(
    keymap: Registry,
    text_input: crate::manager::text_input_registry::TextInputRegistry,
) -> InputResolverChain {
    InputResolverChain::new(vec![
        Box::new(DirectInputResolver::new(keymap)),
        Box::new(TextInputResolver::new(text_input)),
    ])
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;

    #[derive(Debug)]
    enum FixedResult {
        Miss,
        Hit(usize),
    }

    #[derive(Debug)]
    struct FixedResolver(FixedResult);

    impl InputResolver for FixedResolver {
        fn resolve(&self, _context: Context, _key: KeyChord) -> Option<Message> {
            match self.0 {
                FixedResult::Miss => None,
                FixedResult::Hit(index) => Some(Message::GotoBuffer(index)),
            }
        }
    }

    #[test]
    fn chain_preserves_priority_and_falls_through() {
        let chain = InputResolverChain::default()
            .with(FixedResolver(FixedResult::Miss))
            .with(FixedResolver(FixedResult::Hit(2)))
            .with(FixedResolver(FixedResult::Hit(3)));
        assert!(matches!(
            chain.resolve(Context::AppNormal, KeyChord::from(KeyCode::Char('x'))),
            Some(Message::GotoBuffer(2))
        ));
    }
}
