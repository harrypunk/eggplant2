use crate::{data_object::Context, manager::context::ContextBindings};
use std::collections::HashMap;

type ContextBindingMap = HashMap<Context, ContextBindings>;
/// A registry that holds all context key bindings for lookup
#[derive(Default)]
pub struct Registry {
    bindings: ContextBindingMap,
}

impl Registry {
    pub fn add_binding(mut self, binding: ContextBindings) -> Self {
        self.bindings.insert(binding.context, binding);
        self
    }

    pub fn get_binding(&self, ctx: &Context) -> Option<ContextBindings> {
        self.bindings.get(ctx).cloned()
    }
}
