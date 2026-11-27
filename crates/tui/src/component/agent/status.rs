use eggplant_agent::RunStatus;

use super::props::AgentViewProps;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunStatusPresentation {
    Summary,
    Detailed,
}

/// Shared notice priority with presentation-specific run detail and idle text.
pub(super) fn status_text(
    props: &AgentViewProps<'_>,
    presentation: RunStatusPresentation,
    idle: &str,
) -> String {
    props
        .model_notice
        .map(|notice| notice.text().to_string())
        .or_else(|| {
            props
                .storage_error
                .map(|error| format!("Storage warning: {error}"))
        })
        .or_else(|| {
            props
                .session
                .run()
                .map(|run| match (&run.status, presentation) {
                    (RunStatus::Failed(error), RunStatusPresentation::Detailed) => {
                        format!("Failed: {error}")
                    }
                    (status, _) => status.label().to_string(),
                })
        })
        .unwrap_or_else(|| idle.to_string())
}
