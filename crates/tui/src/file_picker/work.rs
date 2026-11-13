use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use editor::{DocumentFactory, OpenDocumentError, preview_document};

use ignore::WalkBuilder;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

use super::{FileEntry, PreviewState, ScanDepth};
use crate::{
    data_object::Message, filesystem::is_built_in_ignored, manager::effect::FeatureEffect,
};

const PREVIEW_VIEWPORT_WIDTH: u16 = 80;
const PREVIEW_VIEWPORT_HEIGHT: u16 = 22;

/// Blocking filesystem/matching work executed outside the terminal event loop.
pub enum FilePickerRequest {
    Scan {
        root: PathBuf,
        depth: ScanDepth,
        revision: u64,
    },
    Filter {
        files: Arc<Vec<FileEntry>>,
        query: String,
        revision: u64,
    },
    Preview {
        documents: DocumentFactory,
        path: PathBuf,
        revision: u64,
    },
}

impl FilePickerRequest {
    pub fn perform(self) -> Message {
        match self {
            Self::Scan {
                root,
                depth,
                revision,
            } => Message::FileScanFinished {
                revision,
                result: scan_files(&root, depth),
            },
            Self::Filter {
                files,
                query,
                revision,
            } => Message::FileFilterFinished {
                revision,
                results: filter_files(&files, &query),
            },
            Self::Preview {
                documents,
                path,
                revision,
            } => Message::FilePreviewFinished {
                revision,
                preview: read_preview(&documents, &path),
            },
        }
    }
}

impl FeatureEffect for FilePickerRequest {
    fn perform(self: Box<Self>) -> Message {
        FilePickerRequest::perform(*self)
    }
}

fn scan_files(root: &Path, depth: ScanDepth) -> Result<Vec<FileEntry>, String> {
    let mut builder = WalkBuilder::new(root);
    builder.max_depth(match depth {
        ScanDepth::TopLevel => Some(1),
        ScanDepth::Recursive => None,
    });
    builder
        .hidden(false)
        .parents(true)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .filter_entry(|entry| entry.depth() == 0 || !is_built_in_ignored(entry.file_name()));

    let mut files = builder
        .build()
        .filter_map(|entry| match entry {
            Ok(entry) if entry.depth() > 0 && entry.path().is_file() => {
                let path = entry.path().to_path_buf();
                let relative = path.strip_prefix(root).unwrap_or(&path);
                Some(Ok(FileEntry {
                    display: relative.to_string_lossy().into_owned(),
                    path,
                }))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error.to_string())),
        })
        .collect::<Result<Vec<_>, String>>()?;
    files.sort_by(|left, right| left.display.cmp(&right.display));
    Ok(files)
}

fn filter_files(files: &[FileEntry], query: &str) -> Vec<FileEntry> {
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut buffer = Vec::new();
    let mut matches = files
        .iter()
        .filter_map(|entry| {
            pattern
                .score(Utf32Str::new(&entry.display, &mut buffer), &mut matcher)
                .map(|score| (score, entry.clone()))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.display.cmp(&right.display))
    });
    matches
        .into_iter()
        .map(|(_, entry)| entry)
        .collect::<Vec<_>>()
}

fn read_preview(documents: &DocumentFactory, path: &Path) -> PreviewState {
    match preview_document(
        documents,
        path,
        PREVIEW_VIEWPORT_WIDTH,
        PREVIEW_VIEWPORT_HEIGHT,
    ) {
        Ok(snapshot) => PreviewState::Text(snapshot),
        Err(OpenDocumentError::NotTextFile) => {
            PreviewState::Unavailable("Not a text file".to_string())
        }
        Err(error) => PreviewState::Unavailable(format!("Preview unavailable: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            display: path.to_string(),
        }
    }

    #[test]
    fn fuzzy_filter_ranks_matches_and_excludes_non_matches() {
        let files = vec![
            entry("foo/bar"),
            entry("bar/foo"),
            entry("foobar"),
            entry("unrelated"),
        ];

        let matches = filter_files(&files, "foo bar");
        let paths = matches
            .iter()
            .map(|entry| entry.display.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, ["bar/foo", "foo/bar", "foobar"]);
    }

    #[test]
    fn scan_respects_gitignore_and_built_in_ignores() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(directory.path().join("ignored.txt"), "ignored").unwrap();
        std::fs::write(directory.path().join("visible.txt"), "visible").unwrap();
        std::fs::create_dir(directory.path().join("node_modules")).unwrap();
        std::fs::write(directory.path().join("node_modules/package.js"), "ignored").unwrap();

        let files = scan_files(directory.path(), ScanDepth::Recursive).unwrap();
        let names = files
            .iter()
            .map(|entry| entry.display.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, [".gitignore", "visible.txt"]);
    }

    #[test]
    fn top_level_scan_defers_nested_files_until_recursive_scan() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nested")).unwrap();
        std::fs::write(directory.path().join("top.txt"), "top").unwrap();
        std::fs::write(directory.path().join("nested/child.txt"), "child").unwrap();

        let top = scan_files(directory.path(), ScanDepth::TopLevel).unwrap();
        assert_eq!(top[0].display, "top.txt");
        assert_eq!(top.len(), 1);
        let recursive = scan_files(directory.path(), ScanDepth::Recursive).unwrap();
        let names = recursive
            .iter()
            .map(|entry| entry.display.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["nested/child.txt", "top.txt"]);
    }

    #[test]
    fn preview_reads_text_and_rejects_binary_content() {
        let directory = tempfile::tempdir().unwrap();
        let text = directory.path().join("note.txt");
        let binary = directory.path().join("image.bin");
        let link = directory.path().join("link.txt");
        std::fs::write(&text, "first\nsecond").unwrap();
        std::fs::write(&binary, [0_u8, 1, 2]).unwrap();
        std::os::unix::fs::symlink(&text, &link).unwrap();
        let documents = DocumentFactory::new();

        let PreviewState::Text(snapshot) = read_preview(&documents, &text) else {
            panic!("text file should produce a view snapshot");
        };
        assert_eq!(snapshot.first_line, 0);
        assert_eq!(snapshot.line_count, 2);
        assert_eq!(snapshot.lines[0].text, "first");
        assert_eq!(snapshot.lines[1].text, "second");
        assert_eq!(
            read_preview(&documents, &binary),
            PreviewState::Unavailable("Not a text file".to_string())
        );
        assert_eq!(
            read_preview(&documents, &link),
            PreviewState::Unavailable("Not a text file".to_string())
        );
    }
}
