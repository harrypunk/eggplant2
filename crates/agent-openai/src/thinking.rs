use std::collections::BTreeMap;

use eggplant_agent::{ThinkingId, ThinkingOption};
use serde_json::Value;

/// OpenAI-compatible reasoning capability and request serialization policy.
pub trait ThinkingRequestPolicy: std::fmt::Debug + Send + Sync {
    fn options(&self) -> &[ThinkingOption];
    fn apply(&self, id: &ThinkingId, body: &mut Value);
}

#[derive(Debug, Default)]
pub struct NoThinkingPolicy;

impl ThinkingRequestPolicy for NoThinkingPolicy {
    fn options(&self) -> &[ThinkingOption] {
        &[]
    }

    fn apply(&self, _id: &ThinkingId, _body: &mut Value) {}
}

#[derive(Debug, Clone)]
pub struct ReasoningEffort {
    pub option: ThinkingOption,
    /// Provider-native value; absent means selection leaves the request unchanged.
    pub wire_value: Option<String>,
}

#[derive(Debug)]
pub struct ReasoningEffortPolicy {
    options: Vec<ThinkingOption>,
    wire_values: BTreeMap<ThinkingId, String>,
}

impl ReasoningEffortPolicy {
    pub fn new(entries: Vec<ReasoningEffort>) -> Self {
        let options = entries.iter().map(|entry| entry.option.clone()).collect();
        let wire_values = entries
            .into_iter()
            .filter_map(|entry| entry.wire_value.map(|value| (entry.option.id, value)))
            .collect();
        Self {
            options,
            wire_values,
        }
    }

    /// Conservative OpenAI-compatible defaults; extended levels require explicit opt-in.
    pub fn standard() -> Self {
        Self::new(vec![
            effort("off", "Off", "No reasoning", None),
            effort(
                "minimal",
                "Minimal",
                "Very brief reasoning",
                Some("minimal"),
            ),
            effort("low", "Low", "Light reasoning", Some("low")),
            effort("medium", "Medium", "Moderate reasoning", Some("medium")),
            effort("high", "High", "Deep reasoning", Some("high")),
        ])
    }
}

impl ThinkingRequestPolicy for ReasoningEffortPolicy {
    fn options(&self) -> &[ThinkingOption] {
        &self.options
    }

    fn apply(&self, id: &ThinkingId, body: &mut Value) {
        if let Some(value) = self.wire_values.get(id) {
            body["reasoning_effort"] = Value::String(value.clone());
        }
    }
}

fn effort(id: &str, label: &str, description: &str, wire_value: Option<&str>) -> ReasoningEffort {
    ReasoningEffort {
        option: ThinkingOption::new(id, label, description),
        wire_value: wire_value.map(str::to_string),
    }
}
