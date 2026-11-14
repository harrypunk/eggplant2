use std::ops::Range;

use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{SearcherBuilder, sinks::UTF8};

const MAX_RESULTS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchLine {
    /// Zero-based document line.
    pub line: usize,
    pub text: String,
    /// UTF-8 byte ranges of every non-overlapping match on this line.
    pub match_ranges: Vec<Range<usize>>,
}

/// Search one UTF-8 text snapshot with ripgrep's smart-case regex engine.
pub fn search_text(text: &str, query: &str) -> Result<Vec<MatchLine>, String> {
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
                let text = line.trim_end_matches(['\r', '\n']);
                let mut match_ranges = Vec::new();
                matcher
                    .find_iter(text.as_bytes(), |matched| {
                        match_ranges.push(matched.start()..matched.end());
                        true
                    })
                    .map_err(std::io::Error::other)?;
                matches.push(MatchLine {
                    line: line_number,
                    text: text.to_string(),
                    match_ranges,
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
        let matches = search_text("hello\nHELLO\nyellow\néclair", "ell").unwrap();
        assert_eq!(
            matches,
            vec![
                MatchLine {
                    line: 0,
                    text: "hello".to_string(),
                    match_ranges: vec![std::ops::Range { start: 1, end: 4 }],
                },
                MatchLine {
                    line: 1,
                    text: "HELLO".to_string(),
                    match_ranges: vec![std::ops::Range { start: 1, end: 4 }],
                },
                MatchLine {
                    line: 2,
                    text: "yellow".to_string(),
                    match_ranges: vec![std::ops::Range { start: 1, end: 4 }],
                },
            ]
        );
        assert_eq!(search_text("Élan\néclair", "éla").unwrap().len(), 1);
        assert_eq!(search_text("hello\nHELLO", "ELL").unwrap().len(), 1);
        assert_eq!(
            search_text("ell ell", "ell").unwrap()[0].match_ranges,
            vec![0..3, 4..7]
        );
    }

    #[test]
    fn empty_queries_and_invalid_regexes_are_safe() {
        assert_eq!(search_text("hello", "").unwrap(), Vec::new());
        assert!(search_text("hello", "[").is_err());
    }
}
