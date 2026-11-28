#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandId(String);

impl CommandId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Pure prompt-command metadata used by completion state and views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCommand {
    id: CommandId,
    name: String,
    title: String,
    description: String,
}

impl PromptCommand {
    pub(crate) fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: CommandId::new(id),
            name: name.into(),
            title: title.into(),
            description: description.into(),
        }
    }

    pub fn id(&self) -> &CommandId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn matches(&self, query: &str) -> bool {
        self.name
            .strip_prefix('/')
            .unwrap_or(&self.name)
            .contains(query)
    }
}
