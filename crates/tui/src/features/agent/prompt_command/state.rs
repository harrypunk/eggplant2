use super::{catalog, metadata::PromptCommand};
use crate::component::list::ListState;

/// Commands offered by the Agent prompt's leading-slash menu.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PromptCommandState {
    list: ListState<PromptCommand>,
    query: String,
}

impl PromptCommandState {
    pub fn selected(&self) -> Option<PromptCommand> {
        self.list.selected_item().cloned()
    }

    pub fn commands(&self, prompt: &str) -> Option<Vec<PromptCommand>> {
        match self.selected() {
            Some(_) => Some(self.list.items().to_vec()),
            None => catalog::shared().filter(prompt),
        }
    }

    pub fn clear(&mut self) {
        self.list = ListState::default();
        self.query.clear();
    }

    pub fn quick_fill(&mut self, prompt: &str, movement: PromptCommandMovement) -> Option<String> {
        let query = self
            .selected()
            .map(|_| self.query.clone())
            .unwrap_or_else(|| prompt.to_string());
        if self.selected().is_none() {
            self.list = ListState::new(catalog::shared().filter(&query)?);
        }
        let selected = match movement {
            PromptCommandMovement::Next => self.list.select_next(),
            PromptCommandMovement::Previous => self.list.select_previous(),
            PromptCommandMovement::Current => self.list.select_current(),
        };
        let name = selected?.name().to_string();
        self.query = query;
        Some(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCommandMovement {
    Next,
    Previous,
    Current,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str) -> PromptCommand {
        catalog::resolve(name).unwrap()
    }

    #[test]
    fn slash_lists_and_live_filters_all_commands() {
        assert_eq!(catalog::shared().filter("/"), Some(catalog::catalog()));
        assert_eq!(
            catalog::shared().filter("/p"),
            Some(vec![command("/provider")])
        );
        assert_eq!(
            catalog::shared().filter("/PRO"),
            Some(vec![command("/provider")])
        );
        assert_eq!(catalog::shared().filter("/unknown"), Some(Vec::new()));
        assert!(catalog::shared().filter(" /p").is_none());
        assert!(catalog::shared().filter("explain /provider").is_none());
    }

    #[test]
    fn complete_typed_commands_trim_whitespace_without_loosening_name_matching() {
        for command in catalog::catalog() {
            assert_eq!(catalog::resolve(command.name()), Some(command.clone()));
            assert_eq!(
                catalog::resolve(&format!("  {}\t", command.name())),
                Some(command)
            );
        }
        assert_eq!(catalog::resolve("/provide"), None);
        assert_eq!(catalog::resolve("/PROVIDER"), None);
        assert_eq!(catalog::resolve("/provider extra"), None);
        assert_eq!(catalog::resolve("provider"), None);
    }

    #[test]
    fn selection_requires_quick_fill_and_navigation_wraps() {
        let mut state = PromptCommandState::default();
        assert_eq!(state.selected(), None);
        assert_eq!(
            state.quick_fill("/", PromptCommandMovement::Current),
            Some("/provider".to_string())
        );
        assert_eq!(state.selected(), Some(command("/provider")));

        for (movement, name, selected) in [
            (PromptCommandMovement::Next, "/model", command("/model")),
            (
                PromptCommandMovement::Next,
                "/thinking",
                command("/thinking"),
            ),
            (
                PromptCommandMovement::Next,
                "/sessions",
                command("/sessions"),
            ),
            (PromptCommandMovement::Next, "/restore", command("/restore")),
            (PromptCommandMovement::Next, "/new", command("/new")),
            (PromptCommandMovement::Next, "/logout", command("/logout")),
            (
                PromptCommandMovement::Next,
                "/provider",
                command("/provider"),
            ),
            (
                PromptCommandMovement::Previous,
                "/logout",
                command("/logout"),
            ),
        ] {
            assert_eq!(
                state.quick_fill("/provider", movement),
                Some(name.to_string())
            );
            assert_eq!(state.selected(), Some(selected));
            assert_eq!(state.commands("/provider"), Some(catalog::catalog()));
        }

        state.clear();
        assert_eq!(state.selected(), None);
        assert_eq!(
            state.commands("/provider"),
            Some(vec![command("/provider")])
        );
    }

    #[test]
    fn quick_fill_does_nothing_without_a_match() {
        let mut state = PromptCommandState::default();
        assert_eq!(
            state.quick_fill("/unknown", PromptCommandMovement::Next),
            None
        );
        assert_eq!(state.selected(), None);
    }
}
