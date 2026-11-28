use crate::component::list::ListState;

/// Commands offered by the Agent prompt's leading-slash menu.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PromptCommandState {
    list: ListState<PromptCommand>,
    query: String,
}

impl PromptCommandState {
    pub fn selected(&self) -> Option<PromptCommand> {
        self.list.selected_item().copied()
    }

    pub fn commands(&self, prompt: &str) -> Option<Vec<PromptCommand>> {
        match self.selected() {
            Some(_) => Some(self.list.items().to_vec()),
            None => filter(prompt),
        }
    }

    pub fn clear(&mut self) {
        self.list = ListState::default();
        self.query.clear();
    }

    pub fn quick_fill(
        &mut self,
        prompt: &str,
        movement: PromptCommandMovement,
    ) -> Option<&'static str> {
        let query = self
            .selected()
            .map(|_| self.query.clone())
            .unwrap_or_else(|| prompt.to_string());
        if self.selected().is_none() {
            self.list = ListState::new(filter(&query)?);
        }
        let selected = match movement {
            PromptCommandMovement::Next => self.list.select_next(),
            PromptCommandMovement::Previous => self.list.select_previous(),
            PromptCommandMovement::Current => self.list.select_current(),
        };
        let name = selected?.name();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCommand {
    ProviderLogin,
    Model,
    Thinking,
    Sessions,
    New,
    Logout,
}

impl PromptCommand {
    pub const ALL: [Self; 6] = [
        Self::ProviderLogin,
        Self::Model,
        Self::Thinking,
        Self::Sessions,
        Self::New,
        Self::Logout,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ProviderLogin => "/provider",
            Self::Model => "/model",
            Self::Thinking => "/thinking",
            Self::Sessions => "/sessions",
            Self::New => "/new",
            Self::Logout => "/logout",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::ProviderLogin => "Provider login",
            Self::Model => "Select model",
            Self::Thinking => "Thinking settings",
            Self::Sessions => "Restore session",
            Self::New => "New session",
            Self::Logout => "Log out",
        }
    }

    /// Resolve a complete typed command independently of menu selection.
    pub fn from_input(input: &str) -> Option<Self> {
        let input = input.trim();
        Self::ALL
            .into_iter()
            .find(|command| command.name() == input)
    }

    fn matches(self, query: &str) -> bool {
        self.name()[1..].contains(query)
    }
}

/// Derive menu candidates directly from the prompt or retained navigation query.
pub fn filter(prompt: &str) -> Option<Vec<PromptCommand>> {
    let query = prompt.strip_prefix('/')?.trim_end().to_ascii_lowercase();
    Some(
        PromptCommand::ALL
            .into_iter()
            .filter(|command| command.matches(&query))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_lists_and_live_filters_all_commands() {
        assert_eq!(filter("/"), Some(PromptCommand::ALL.to_vec()));
        assert_eq!(filter("/p"), Some(vec![PromptCommand::ProviderLogin]));
        assert_eq!(filter("/PRO"), Some(vec![PromptCommand::ProviderLogin]));
        assert_eq!(filter("/unknown"), Some(Vec::new()));
        assert!(filter(" /p").is_none());
        assert!(filter("explain /provider").is_none());
    }

    #[test]
    fn complete_typed_commands_trim_whitespace_without_loosening_name_matching() {
        for command in PromptCommand::ALL {
            assert_eq!(PromptCommand::from_input(command.name()), Some(command));
            assert_eq!(
                PromptCommand::from_input(&format!("  {}\t", command.name())),
                Some(command)
            );
        }
        assert_eq!(PromptCommand::from_input("/provide"), None);
        assert_eq!(PromptCommand::from_input("/PROVIDER"), None);
        assert_eq!(PromptCommand::from_input("/provider extra"), None);
        assert_eq!(PromptCommand::from_input("provider"), None);
    }

    #[test]
    fn selection_requires_quick_fill_and_navigation_wraps() {
        let mut state = PromptCommandState::default();
        assert_eq!(state.selected(), None);
        assert_eq!(
            state.quick_fill("/", PromptCommandMovement::Current),
            Some("/provider")
        );
        assert_eq!(state.selected(), Some(PromptCommand::ProviderLogin));

        for (movement, name, selected) in [
            (PromptCommandMovement::Next, "/model", PromptCommand::Model),
            (
                PromptCommandMovement::Next,
                "/thinking",
                PromptCommand::Thinking,
            ),
            (
                PromptCommandMovement::Next,
                "/sessions",
                PromptCommand::Sessions,
            ),
            (PromptCommandMovement::Next, "/new", PromptCommand::New),
            (
                PromptCommandMovement::Next,
                "/logout",
                PromptCommand::Logout,
            ),
            (
                PromptCommandMovement::Next,
                "/provider",
                PromptCommand::ProviderLogin,
            ),
            (
                PromptCommandMovement::Previous,
                "/logout",
                PromptCommand::Logout,
            ),
        ] {
            assert_eq!(state.quick_fill("/provider", movement), Some(name));
            assert_eq!(state.selected(), Some(selected));
            assert_eq!(
                state.commands("/provider"),
                Some(PromptCommand::ALL.to_vec())
            );
        }

        state.clear();
        assert_eq!(state.selected(), None);
        assert_eq!(
            state.commands("/provider"),
            Some(vec![PromptCommand::ProviderLogin])
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
