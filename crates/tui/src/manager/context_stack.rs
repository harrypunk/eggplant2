use std::ops::Deref;

use crate::data_object::Context;

/// LIFO stack of transient workflows with explicit lifetimes.
///
/// Shell state such as focus, active page, editor mode, and Agent panel mode does not belong here.
/// The top frame owns transient input; `pop_if` preserves that ownership discipline.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ContextStack(Vec<Context>);

impl ContextStack {
    pub fn push(&mut self, context: Context) {
        self.0.push(context);
    }

    pub fn pop(&mut self) -> Option<Context> {
        self.0.pop()
    }

    pub fn pop_if(&mut self, predicate: impl FnOnce(&mut Context) -> bool) -> Option<Context> {
        self.0.pop_if(predicate)
    }

    pub fn last(&self) -> Option<&Context> {
        self.0.last()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Deref for ContextStack {
    type Target = [Context];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pop_if_removes_only_the_matching_top_frame() {
        let mut stack = ContextStack::default();
        stack.push(Context::FilePicker);
        stack.push(Context::ThemePicker);

        assert_eq!(stack.pop_if(|active| *active == Context::FilePicker), None);
        assert_eq!(stack.last(), Some(&Context::ThemePicker));
        assert_eq!(
            stack.pop_if(|active| *active == Context::ThemePicker),
            Some(Context::ThemePicker)
        );
        assert_eq!(stack.last(), Some(&Context::FilePicker));
        assert_eq!(stack.pop(), Some(Context::FilePicker));
        assert!(stack.is_empty());
    }
}
