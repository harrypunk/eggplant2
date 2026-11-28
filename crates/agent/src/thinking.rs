#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThinkingId(String);

impl ThinkingId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ThinkingId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

/// Owned display metadata supplied by a provider capability policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingOption {
    pub id: ThinkingId,
    pub label: String,
    pub description: String,
}

impl ThinkingOption {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: ThinkingId::new(id),
            label: label.into(),
            description: description.into(),
        }
    }
}
