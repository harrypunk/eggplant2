use crate::{ProviderMessage, RunId};

/// One answered user/assistant exchange, independent of presentation or storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationTurn {
    pub run_id: RunId,
    pub user: String,
    pub answer: String,
}

impl ConversationTurn {
    fn into_provider_messages(self) -> [ProviderMessage; 2] {
        [
            ProviderMessage::user(self.user),
            ProviderMessage::assistant(self.answer, Vec::new()),
        ]
    }

    fn provider_bytes(&self) -> usize {
        self.user.len().saturating_add(self.answer.len())
    }
}

/// Provider history plus explicit omission metadata; the current prompt is never included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextProjection {
    pub history: Vec<ProviderMessage>,
    pub omitted_turns: usize,
}

/// Selects provider context from answered turns without owning session or request state.
pub trait ConversationContextPolicy: std::fmt::Debug + Send + Sync {
    fn project(&self, turns: &[ConversationTurn]) -> ContextProjection;
}

#[derive(Debug, Default)]
pub struct FullContext;

impl ConversationContextPolicy for FullContext {
    fn project(&self, turns: &[ConversationTurn]) -> ContextProjection {
        ContextProjection {
            history: turns
                .iter()
                .cloned()
                .flat_map(ConversationTurn::into_provider_messages)
                .collect(),
            omitted_turns: 0,
        }
    }
}

/// Deterministic whole-turn limits. Text is never truncated or synthesized.
#[derive(Debug)]
pub struct BoundedContext {
    max_turns: usize,
    max_bytes: usize,
}

impl BoundedContext {
    pub fn new(max_turns: usize, max_bytes: usize) -> Self {
        Self {
            max_turns,
            max_bytes,
        }
    }
}

impl ConversationContextPolicy for BoundedContext {
    fn project(&self, turns: &[ConversationTurn]) -> ContextProjection {
        let mut selected = turns
            .iter()
            .rev()
            .scan(0usize, |used, turn| {
                *used = used.saturating_add(turn.provider_bytes());
                Some(*used)
            })
            .zip(turns.iter().rev())
            .take(self.max_turns)
            .take_while(|(bytes, _)| *bytes <= self.max_bytes)
            .map(|(_, turn)| turn.clone())
            .collect::<Vec<_>>();
        selected.reverse();
        let omitted_turns = turns.len().saturating_sub(selected.len());
        ContextProjection {
            history: selected
                .into_iter()
                .flat_map(ConversationTurn::into_provider_messages)
                .collect(),
            omitted_turns,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(id: u64, user: &str, answer: &str) -> ConversationTurn {
        ConversationTurn {
            run_id: RunId(id),
            user: user.to_string(),
            answer: answer.to_string(),
        }
    }

    #[test]
    fn bounded_context_keeps_latest_complete_turns_and_reports_omissions() {
        let turns = vec![
            turn(1, "first", "answer one"),
            turn(2, "second", "answer two"),
            turn(3, "third", "answer three"),
        ];

        let projection = BoundedContext::new(2, usize::MAX).project(&turns);

        assert_eq!(projection.omitted_turns, 1);
        assert_eq!(
            projection.history,
            vec![
                ProviderMessage::user("second".to_string()),
                ProviderMessage::assistant("answer two".to_string(), Vec::new()),
                ProviderMessage::user("third".to_string()),
                ProviderMessage::assistant("answer three".to_string(), Vec::new()),
            ]
        );
    }

    #[test]
    fn byte_budget_never_truncates_or_includes_an_oversized_turn() {
        let turns = vec![turn(2, "too many", "bytes here"), turn(1, "keep", "latest")];
        let projection = BoundedContext::new(2, 11).project(&turns);
        assert_eq!(projection.omitted_turns, 1);
        assert_eq!(
            projection.history,
            vec![
                ProviderMessage::user("keep".to_string()),
                ProviderMessage::assistant("latest".to_string(), Vec::new()),
            ]
        );

        let projection = BoundedContext::new(2, 1).project(&turns);
        assert_eq!(projection.omitted_turns, 2);
        assert!(projection.history.is_empty());
    }
}
