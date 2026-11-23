/// Commands offered by the Agent prompt's leading-slash menu.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PromptCommandState {
    selected: Option<PromptCommand>,
    query: String,
}

impl PromptCommandState {
    pub fn selected(&self) -> Option<PromptCommand> {
        self.selected
    }

    pub fn commands(&self, prompt: &str) -> Option<Vec<PromptCommand>> {
        match self.selected {
            Some(_) => filter(&self.query),
            None => filter(prompt),
        }
    }

    pub fn clear(&mut self) {
        self.selected = None;
        self.query.clear();
    }

    pub fn quick_fill(
        &mut self,
        prompt: &str,
        movement: PromptCommandMovement,
    ) -> Option<&'static str> {
        let query = self
            .selected
            .map(|_| self.query.clone())
            .unwrap_or_else(|| prompt.to_string());
        let commands = filter(&query)?;
        if commands.is_empty() {
            return None;
        }
        let current = self
            .selected
            .and_then(|selected| commands.iter().position(|command| *command == selected));
        let index = match movement {
            PromptCommandMovement::Next => current.map_or(0, |index| (index + 1) % commands.len()),
            PromptCommandMovement::Previous => current.map_or(commands.len() - 1, |index| {
                (index + commands.len() - 1) % commands.len()
            }),
            PromptCommandMovement::Current => current.unwrap_or(0),
        };
        let selected = commands[index];
        self.selected = Some(selected);
        self.query = query;
        Some(selected.name())
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
    Logout,
}

impl PromptCommand {
    pub const ALL: [Self; 4] = [
        Self::ProviderLogin,
        Self::Model,
        Self::Thinking,
        Self::Logout,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ProviderLogin => "/provider",
            Self::Model => "/model",
            Self::Thinking => "/thinking",
            Self::Logout => "/logout",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::ProviderLogin => "Provider login",
            Self::Model => "Select model",
            Self::Thinking => "Thinking settings",
            Self::Logout => "Log out",
        }
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
