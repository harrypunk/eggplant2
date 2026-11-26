/// Provider-reported tokens for one model round.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

impl TokenUsage {
    pub(crate) fn accumulate(&mut self, usage: Self) {
        self.input = self.input.saturating_add(usage.input);
        self.output = self.output.saturating_add(usage.output);
        self.total = self.total.saturating_add(usage.total);
    }
}
