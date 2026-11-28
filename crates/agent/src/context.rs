use crate::{ProviderMessage, RunId, TokenUsage};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextCapacity {
    pub input_tokens: u64,
}

impl ContextCapacity {
    pub fn new(input_tokens: u64) -> Self {
        Self { input_tokens }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenInput<'a> {
    pub turns: &'a [ConversationTurn],
    pub history: &'a [ProviderMessage],
    pub prompt: &'a str,
}

/// Optional provider-neutral token measurement. Unknown is preferable to an estimate.
pub trait TokenEstimator: std::fmt::Debug + Send + Sync {
    fn estimate(&self, input: TokenInput<'_>) -> Option<u64>;
}

#[derive(Debug, Clone, Copy)]
pub struct RequestContext<'a> {
    pub turns: &'a [ConversationTurn],
    pub history: &'a [ProviderMessage],
    pub prompt: &'a str,
    pub capacity: Option<ContextCapacity>,
    pub estimator: Option<&'a dyn TokenEstimator>,
    pub measured_usage: Option<TokenUsage>,
}

impl<'a> RequestContext<'a> {
    pub fn new(turns: &'a [ConversationTurn], prompt: &'a str) -> Self {
        Self {
            turns,
            history: &[],
            prompt,
            capacity: None,
            estimator: None,
            measured_usage: None,
        }
    }

    pub fn with_follow_up(mut self, history: &'a [ProviderMessage]) -> Self {
        self.history = history;
        self
    }

    pub fn with_capacity(mut self, capacity: ContextCapacity) -> Self {
        self.capacity = Some(capacity);
        self
    }

    pub fn with_estimator(mut self, estimator: &'a dyn TokenEstimator) -> Self {
        self.estimator = Some(estimator);
        self
    }

    pub fn with_measured_usage(mut self, usage: TokenUsage) -> Self {
        self.measured_usage = Some(usage);
        self
    }

    fn token_input(self, turns: &'a [ConversationTurn]) -> TokenInput<'a> {
        TokenInput {
            turns,
            history: self.history,
            prompt: self.prompt,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContextMeasurement {
    pub input_bytes: usize,
    pub estimated_input_tokens: Option<u64>,
    pub measured_input_tokens: Option<u64>,
    pub capacity_tokens: Option<u64>,
    pub fits_capacity: Option<bool>,
}

/// Provider history plus explicit omission and measurement metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextProjection {
    pub history: Vec<ProviderMessage>,
    pub omitted_turns: usize,
    pub measurement: ContextMeasurement,
}

/// Selects provider context without owning session or request state.
pub trait ConversationContextPolicy: std::fmt::Debug + Send + Sync {
    fn project_request(&self, request: RequestContext<'_>) -> ContextProjection;

    fn project(&self, turns: &[ConversationTurn]) -> ContextProjection {
        self.project_request(RequestContext::new(turns, ""))
    }
}

#[derive(Debug, Default)]
pub struct FullContext;

impl ConversationContextPolicy for FullContext {
    fn project_request(&self, request: RequestContext<'_>) -> ContextProjection {
        let history = match request.history.is_empty() {
            true => request
                .turns
                .iter()
                .cloned()
                .flat_map(ConversationTurn::into_provider_messages)
                .collect(),
            false => request.history.to_vec(),
        };
        ContextProjection {
            omitted_turns: 0,
            measurement: measure(request, request.turns, &history),
            history,
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

    fn fits(&self, request: &RequestContext<'_>, turns: &[ConversationTurn]) -> bool {
        let bytes = turns
            .iter()
            .map(ConversationTurn::provider_bytes)
            .sum::<usize>()
            .saturating_add(request.prompt.len());
        if bytes > self.max_bytes {
            return false;
        }
        match (request.capacity, request.estimator) {
            (Some(capacity), Some(estimator)) => estimator
                .estimate(request.token_input(turns))
                .is_none_or(|tokens| tokens <= capacity.input_tokens),
            _ => true,
        }
    }
}

impl ConversationContextPolicy for BoundedContext {
    fn project_request(&self, request: RequestContext<'_>) -> ContextProjection {
        if !request.history.is_empty() {
            return ContextProjection {
                omitted_turns: 0,
                measurement: measure(request, &[], request.history),
                history: request.history.to_vec(),
            };
        }
        let first = request.turns.len().saturating_sub(self.max_turns);
        let start = (first..=request.turns.len())
            .find(|start| self.fits(&request, &request.turns[*start..]))
            .unwrap_or(request.turns.len());
        let selected = &request.turns[start..];
        let history = selected
            .iter()
            .cloned()
            .flat_map(ConversationTurn::into_provider_messages)
            .collect::<Vec<_>>();
        ContextProjection {
            omitted_turns: start,
            measurement: measure(request, selected, &history),
            history,
        }
    }
}

fn measure(
    request: RequestContext<'_>,
    turns: &[ConversationTurn],
    history: &[ProviderMessage],
) -> ContextMeasurement {
    let history_bytes = match history.is_empty() {
        true => turns
            .iter()
            .map(ConversationTurn::provider_bytes)
            .sum::<usize>(),
        false => history.iter().map(provider_message_bytes).sum::<usize>(),
    };
    let estimated_input_tokens = request.estimator.and_then(|estimator| {
        estimator.estimate(TokenInput {
            turns: match request.history.is_empty() {
                true => turns,
                false => &[],
            },
            history,
            prompt: request.prompt,
        })
    });
    let capacity_tokens = request.capacity.map(|capacity| capacity.input_tokens);
    ContextMeasurement {
        input_bytes: history_bytes.saturating_add(request.prompt.len()),
        estimated_input_tokens,
        measured_input_tokens: request.measured_usage.map(|usage| usage.input),
        capacity_tokens,
        fits_capacity: capacity_tokens
            .zip(estimated_input_tokens)
            .map(|(capacity, estimate)| estimate <= capacity),
    }
}

fn provider_message_bytes(message: &ProviderMessage) -> usize {
    message
        .content
        .len()
        .saturating_add(message.tool_call_id.as_ref().map_or(0, String::len))
        .saturating_add(
            message
                .tool_calls
                .iter()
                .map(|call| {
                    call.id
                        .len()
                        .saturating_add(call.name.len())
                        .saturating_add(call.arguments.len())
                })
                .sum::<usize>(),
        )
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

    #[derive(Debug)]
    struct ExactTestEstimator;

    impl TokenEstimator for ExactTestEstimator {
        fn estimate(&self, input: TokenInput<'_>) -> Option<u64> {
            let history = match input.history.is_empty() {
                true => input
                    .turns
                    .iter()
                    .map(ConversationTurn::provider_bytes)
                    .sum::<usize>(),
                false => input
                    .history
                    .iter()
                    .map(provider_message_bytes)
                    .sum::<usize>(),
            };
            Some((history + input.prompt.len()) as u64)
        }
    }

    #[test]
    fn token_capacity_uses_the_injected_estimator_without_touching_follow_ups() {
        let turns = vec![
            turn(1, "oversized older turn", "with many bytes"),
            turn(2, "keep", "latest"),
        ];
        let estimator = ExactTestEstimator;
        let request = RequestContext::new(&turns, "")
            .with_capacity(ContextCapacity::new(10))
            .with_estimator(&estimator)
            .with_measured_usage(TokenUsage {
                input: 7,
                output: 3,
                total: 10,
            });
        let projection = BoundedContext::new(4, usize::MAX).project_request(request);

        assert_eq!(projection.omitted_turns, 1);
        assert_eq!(projection.measurement.estimated_input_tokens, Some(10));
        assert_eq!(projection.measurement.measured_input_tokens, Some(7));
        assert_eq!(projection.measurement.capacity_tokens, Some(10));
        assert_eq!(projection.measurement.fits_capacity, Some(true));

        let follow_up = vec![ProviderMessage::user("full tool history".to_string())];
        let projection = BoundedContext::new(0, 0).project_request(
            RequestContext::new(&turns, "")
                .with_follow_up(&follow_up)
                .with_capacity(ContextCapacity::new(1))
                .with_estimator(&estimator),
        );
        assert_eq!(projection.omitted_turns, 0);
        assert_eq!(projection.history, follow_up);
        assert_eq!(projection.measurement.fits_capacity, Some(false));
    }
}
