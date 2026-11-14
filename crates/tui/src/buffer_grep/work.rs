use std::sync::Arc;

use editor::BufferTextSnapshot;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{SearcherBuilder, sinks::UTF8};

use super::GrepMatch;
use crate::{
    data_object::{Message, Model},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

const MAX_RESULTS: usize = 1_000;

pub struct BufferGrepRequest {
    source: Arc<BufferTextSnapshot>,
    query: String,
    query_revision: u64,
}

impl BufferGrepRequest {
    pub fn new(source: Arc<BufferTextSnapshot>, query: String, query_revision: u64) -> Self {
        Self {
            source,
            query,
            query_revision,
        }
    }

    pub fn perform(self) -> Message {
        let result = search_buffer(&self.source.text, &self.query);
        Message::FeatureCompletion(FeatureCompletion::new(BufferGrepCompletion {
            source: (self.source.buffer_id, self.source.revision),
            query_revision: self.query_revision,
            result,
        }))
    }
}

impl FeatureEffect for BufferGrepRequest {
    fn perform(self: Box<Self>) -> Message {
        BufferGrepRequest::perform(*self)
    }
}

struct BufferGrepCompletion {
    source: (editor::BufferId, i32),
    query_revision: u64,
    result: Result<Vec<GrepMatch>, String>,
}

impl Completion for BufferGrepCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let active_revision = model.editor.active_revision();
        model.buffer_grep.complete(
            self.query_revision,
            self.source,
            active_revision,
            self.result,
        );
        None
    }
}

fn search_buffer(text: &str, query: &str) -> Result<Vec<GrepMatch>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let matcher = RegexMatcherBuilder::new()
        .case_smart(true)
        .build(query)
        .map_err(|error| error.to_string())?;
    let mut matches = Vec::new();
    SearcherBuilder::new()
        .line_number(true)
        .build()
        .search_slice(
            &matcher,
            text.as_bytes(),
            UTF8(|line_number, line| {
                let line_number =
                    usize::try_from(line_number.saturating_sub(1)).unwrap_or(usize::MAX);
                matches.push(GrepMatch {
                    line: line_number,
                    text: line.trim_end_matches(['\r', '\n']).to_string(),
                });
                Ok(matches.len() < MAX_RESULTS)
            }),
        )
        .map_err(|error| error.to_string())?;
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_lines_with_smart_case_and_unicode_text() {
        let matches = search_buffer("hello\nHELLO\nyellow\néclair", "ell").unwrap();
        assert_eq!(
            matches,
            vec![
                GrepMatch {
                    line: 0,
                    text: "hello".to_string(),
                },
                GrepMatch {
                    line: 1,
                    text: "HELLO".to_string(),
                },
                GrepMatch {
                    line: 2,
                    text: "yellow".to_string(),
                },
            ]
        );
        assert_eq!(search_buffer("Élan\néclair", "éla").unwrap().len(), 1);
        assert_eq!(search_buffer("hello\nHELLO", "ELL").unwrap().len(), 1);
    }

    #[test]
    fn empty_queries_and_invalid_regexes_are_safe() {
        assert_eq!(search_buffer("hello", "").unwrap(), Vec::new());
        assert!(search_buffer("hello", "[").is_err());
    }
}
