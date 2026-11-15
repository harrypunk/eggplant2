use std::path::{Path, PathBuf};

use editor::{DocumentFactory, TextViewSnapshot, preview_document_at, read_text_file};

use super::ProjectGrepResult;
use crate::{
    data_object::{Message, Model},
    filesystem::workspace_walk,
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    search::{MatchLine, TextSearcher},
};

use super::state::PREVIEW_HEIGHT;

const MAX_RESULTS: usize = 1_000;

pub enum ProjectGrepRequest {
    Search {
        root: PathBuf,
        query: String,
        query_revision: u64,
    },
    Preview {
        documents: DocumentFactory,
        path: PathBuf,
        line: usize,
        query_revision: u64,
        preview_revision: u64,
    },
}

impl ProjectGrepRequest {
    pub fn perform(self) -> Message {
        let completion = match self {
            Self::Search {
                root,
                query,
                query_revision,
            } => ProjectGrepCompletion::Search {
                query_revision,
                result: search_project(&root, &query),
            },
            Self::Preview {
                documents,
                path,
                line,
                query_revision,
                preview_revision,
            } => ProjectGrepCompletion::Preview {
                query_revision,
                preview_revision,
                result: preview_document_at(&documents, &path, line, PREVIEW_HEIGHT)
                    .map_err(|error| error.to_string()),
            },
        };
        Message::FeatureCompletion(FeatureCompletion::new(completion))
    }
}

impl FeatureEffect for ProjectGrepRequest {
    fn perform(self: Box<Self>) -> Message {
        ProjectGrepRequest::perform(*self)
    }
}

enum ProjectGrepCompletion {
    Search {
        query_revision: u64,
        result: Result<Vec<ProjectGrepResult>, String>,
    },
    Preview {
        query_revision: u64,
        preview_revision: u64,
        result: Result<TextViewSnapshot, String>,
    },
}

impl Completion for ProjectGrepCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match *self {
            Self::Search {
                query_revision,
                result,
            } => model
                .project_grep
                .complete_search(query_revision, result)
                .map(Effect::feature),
            Self::Preview {
                query_revision,
                preview_revision,
                result,
            } => {
                model
                    .project_grep
                    .complete_preview(query_revision, preview_revision, result);
                None
            }
        }
    }
}

fn search_project(root: &Path, query: &str) -> Result<Vec<ProjectGrepResult>, String> {
    let searcher = TextSearcher::new(query)?;
    let mut results = Vec::new();
    for entry in workspace_walk(root).build() {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.depth() == 0 || !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let Ok(text) = read_text_file(path) else {
            continue;
        };
        let display = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        for matched in searcher.search(&text)? {
            results.push(ProjectGrepResult::from_match(path, &display, matched));
            if results.len() == MAX_RESULTS {
                results.sort_by(project_order);
                return Ok(results);
            }
        }
    }
    results.sort_by(project_order);
    Ok(results)
}

impl ProjectGrepResult {
    fn from_match(path: &Path, display: &str, matched: MatchLine) -> Self {
        Self {
            path: path.to_path_buf(),
            display: display.to_string(),
            line: matched.line,
            text: matched.text,
            match_ranges: matched.match_ranges,
        }
    }
}

fn project_order(left: &ProjectGrepResult, right: &ProjectGrepResult) -> std::cmp::Ordering {
    left.display
        .cmp(&right.display)
        .then_with(|| left.line.cmp(&right.line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_workspace_text_with_shared_ignore_and_file_rules() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nested")).unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(directory.path().join("nested/note.txt"), "hello\nHELLO\n").unwrap();
        std::fs::write(directory.path().join("binary.bin"), [0, 1, 2]).unwrap();
        std::fs::write(directory.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(directory.path().join("ignored.txt"), "hello\n").unwrap();

        let results = search_project(directory.path(), "ell").unwrap();

        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|result| result.display == "nested/note.txt")
        );
        assert_eq!(results[0].line, 0);
        assert_eq!(results[1].line, 1);
        assert!(results.iter().all(|result| !result.match_ranges.is_empty()));
        assert!(search_project(directory.path(), "[").is_err());
    }
}
