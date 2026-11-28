use std::{collections::BTreeMap, fmt, sync::Arc};

use editor::EditorChange;
use eggplant_agent::{ToolCall, ToolConcurrency, ToolDescriptor, ToolId, ToolResult};

use super::{host::AgentHost, tools::ToolContext};

#[derive(Debug)]
pub struct ToolOutcome {
    pub result: ToolResult,
    pub change: Option<EditorChange>,
}

impl ToolOutcome {
    pub fn immediate(result: ToolResult) -> Self {
        Self {
            result,
            change: None,
        }
    }

    pub fn edited(result: ToolResult, change: EditorChange) -> Self {
        Self {
            result,
            change: Some(change),
        }
    }
}

#[derive(Debug)]
pub enum PreparedTool {
    Immediate(ToolResult),
    MainThread(Box<dyn MainThreadTool>),
}

pub trait MainThreadTool: std::fmt::Debug + Send {
    fn execute(self: Box<Self>, host: &mut dyn AgentHost) -> ToolOutcome;
}

pub trait AgentTool: std::fmt::Debug + Send + Sync {
    fn descriptor(&self) -> &ToolDescriptor;

    fn prepare(&self, call: &ToolCall, context: &ToolContext) -> Result<PreparedTool, ToolResult>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRegistryError(String);

impl ToolRegistryError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ToolRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ToolRegistryError {}

#[derive(Debug)]
pub struct ToolRegistry {
    tools: BTreeMap<ToolId, Arc<dyn AgentTool>>,
    max_argument_bytes: usize,
}

impl ToolRegistry {
    pub fn new(
        tools: impl IntoIterator<Item = Arc<dyn AgentTool>>,
    ) -> Result<Self, ToolRegistryError> {
        let tools = tools
            .into_iter()
            .map(|tool| (tool.descriptor().id.clone(), tool))
            .collect::<Vec<_>>();
        let ids = tools
            .iter()
            .map(|(id, _)| id)
            .collect::<std::collections::BTreeSet<_>>();
        if ids.len() != tools.len() {
            return Err(ToolRegistryError::new("duplicate agent tool ID"));
        }
        let valid = tools.iter().all(|(_, tool)| {
            let descriptor = tool.descriptor();
            !descriptor.id.as_str().is_empty() && !descriptor.description.trim().is_empty()
        });
        if !valid {
            return Err(ToolRegistryError::new(
                "agent tools require non-empty IDs and descriptions",
            ));
        }
        Ok(Self {
            tools: tools.into_iter().collect(),
            max_argument_bytes: super::tools::MAX_ARGUMENT_BYTES,
        })
    }

    pub fn standard() -> Self {
        Self::new([
            Arc::new(super::tools::ReadTool::new()) as Arc<dyn AgentTool>,
            Arc::new(super::tools::GrepTool::new()) as Arc<dyn AgentTool>,
            Arc::new(super::write::ExactReplacement::new()) as Arc<dyn AgentTool>,
            Arc::new(super::create::FileCreation::new()) as Arc<dyn AgentTool>,
        ])
        .expect("built-in agent tools must have unique valid descriptors")
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools
            .values()
            .map(|tool| tool.descriptor().clone())
            .collect()
    }

    pub fn validate_round(&self, calls: &[ToolCall]) -> Result<(), String> {
        let call_ids = calls
            .iter()
            .map(|call| call.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let valid = call_ids.len() == calls.len()
            && calls.iter().all(|call| {
                !call.id.is_empty()
                    && !call.name.is_empty()
                    && call.arguments.len() <= self.max_argument_bytes
            });
        if !valid {
            return Err("provider emitted an invalid tool call".to_string());
        }
        let exclusive = calls.iter().any(|call| {
            self.tools
                .get(&ToolId::from(call.name.as_str()))
                .is_some_and(|tool| tool.descriptor().concurrency == ToolConcurrency::Exclusive)
        });
        if exclusive && calls.len() != 1 {
            return Err("exclusive tool must be the only tool call in its round".to_string());
        }
        Ok(())
    }

    pub fn prepare(&self, call: &ToolCall, context: &ToolContext) -> PreparedTool {
        if call.arguments.len() > self.max_argument_bytes {
            return PreparedTool::Immediate(super::tools::error(
                call,
                "tool arguments exceed the byte limit".to_string(),
            ));
        }
        self.tools
            .get(&ToolId::from(call.name.as_str()))
            .map_or_else(
                || {
                    PreparedTool::Immediate(super::tools::error(
                        call,
                        format!("unknown tool `{}`", call.name),
                    ))
                },
                |tool| match (tool.prepare(call, context), tool.descriptor().concurrency) {
                    (Ok(PreparedTool::MainThread(_)), ToolConcurrency::Shared) => {
                        PreparedTool::Immediate(super::tools::error(
                            call,
                            "main-thread tools must declare exclusive concurrency".to_string(),
                        ))
                    }
                    (Ok(prepared), _) => prepared,
                    (Err(result), _) => PreparedTool::Immediate(result),
                },
            )
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::standard()
    }
}

#[cfg(test)]
mod tests {
    use eggplant_agent::ToolResultStatus;

    use super::*;

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: "{}".to_string(),
        }
    }

    #[derive(Debug)]
    struct SharedMainThread {
        descriptor: ToolDescriptor,
    }

    impl SharedMainThread {
        fn new() -> Self {
            Self {
                descriptor: ToolDescriptor::new(
                    "shared-main",
                    "Invalid shared main-thread tool",
                    serde_json::json!({"type": "object"}),
                ),
            }
        }
    }

    impl AgentTool for SharedMainThread {
        fn descriptor(&self) -> &ToolDescriptor {
            &self.descriptor
        }

        fn prepare(
            &self,
            _call: &ToolCall,
            _context: &ToolContext,
        ) -> Result<PreparedTool, ToolResult> {
            Ok(PreparedTool::MainThread(Box::new(UnusedMainThread)))
        }
    }

    #[derive(Debug)]
    struct UnusedMainThread;

    impl MainThreadTool for UnusedMainThread {
        fn execute(self: Box<Self>, _host: &mut dyn AgentHost) -> ToolOutcome {
            panic!("invalid shared main-thread tool must not execute")
        }
    }

    #[test]
    fn standard_descriptors_drive_shared_and_exclusive_round_policy() {
        let registry = ToolRegistry::standard();
        let descriptors = registry.descriptors();
        let ids = descriptors
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids, ["create", "grep", "read", "write"].into());
        assert!(
            descriptors
                .iter()
                .filter(|descriptor| { matches!(descriptor.id.as_str(), "write" | "create") })
                .all(|descriptor| descriptor.concurrency == ToolConcurrency::Exclusive)
        );
        assert!(registry.validate_round(&[call("write-1", "write")]).is_ok());
        assert!(
            registry
                .validate_round(&[call("read-1", "read"), call("write-1", "write")])
                .is_err()
        );
        assert!(
            registry
                .validate_round(&[call("same", "read"), call("same", "grep")])
                .is_err()
        );
    }

    #[test]
    fn unknown_tools_are_results_and_duplicate_registration_is_rejected() {
        let registry = ToolRegistry::standard();
        let prepared = registry.prepare(&call("unknown-1", "shell"), &ToolContext::default());
        let PreparedTool::Immediate(result) = prepared else {
            panic!("unknown tool must not execute");
        };
        assert_eq!(result.status, ToolResultStatus::Error);

        let duplicate = ToolRegistry::new([
            Arc::new(super::super::tools::ReadTool::new()) as Arc<dyn AgentTool>,
            Arc::new(super::super::tools::ReadTool::new()) as Arc<dyn AgentTool>,
        ]);
        assert!(duplicate.is_err());
    }

    #[test]
    fn shared_main_thread_tools_are_rejected_before_execution() {
        let registry =
            ToolRegistry::new([Arc::new(SharedMainThread::new()) as Arc<dyn AgentTool>]).unwrap();

        let prepared = registry.prepare(&call("shared-1", "shared-main"), &ToolContext::default());

        let PreparedTool::Immediate(result) = prepared else {
            panic!("invalid tool must return an error");
        };
        assert_eq!(result.status, ToolResultStatus::Error);
        assert!(result.content.contains("exclusive"));
    }
}
