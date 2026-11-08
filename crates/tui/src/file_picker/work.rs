use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use ignore::WalkBuilder;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

use super::{FileEntry, PreviewState};
use crate::{data_object::Message, filesystem::is_built_in_ignored};

const PREVIEW_BYTE_LIMIT: u64 = 64 * 1024;

/// Blocking filesystem/matching work executed outside the terminal event loop.
pub enum FilePickerRequest {
    Scan {
        root: PathBuf,
        revision: u64,
    },
    Filter {
        files: Arc<Vec<FileEntry>>,
        query: String,
        revision: u64,
    },
    Preview {
        path: PathBuf,
        revision: u64,
    },
}

impl FilePickerRequest {
    pub fn perform(self) -> Message {
        match self {
            Self::Scan { root, revision } => Message::FileScanFinished {
                revision,
                result: scan_files(&root),
            },
            Self::Filter {
                files,
                query,
                revision,
            } => Message::FileFilterFinished {
                revision,
                results: filter_files(&files, &query),
            },
            Self::Preview { path, revision } => Message::FilePreviewFinished {
                revision,
                preview: read_preview(&path),
            },
        }
    }
}

fn scan_files(root: &Path) -> Result<Vec<FileEntry>, String> {
    let mut builder = WalkBuilder::new(root);
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

fn read_preview(path: &Path) -> PreviewState {
    let mut bytes = Vec::new();
    let result =
        File::open(path).and_then(|file| file.take(PREVIEW_BYTE_LIMIT).read_to_end(&mut bytes));
    if let Err(error) = result {
        return PreviewState::Unavailable(format!("Preview unavailable: {error}"));
    }
    if bytes.contains(&0) {
        return PreviewState::Unavailable("Binary file".to_string());
    }
    match String::from_utf8(bytes) {
        Ok(text) => {
            PreviewState::Text(text.lines().map(ToOwned::to_owned).collect::<Vec<String>>())
        }
        Err(_) => PreviewState::Unavailable("Preview requires UTF-8 text".to_string()),
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

        let files = scan_files(directory.path()).unwrap();
        let names = files
            .iter()
            .map(|entry| entry.display.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, [".gitignore", "visible.txt"]);
    }

    #[test]
    fn preview_reads_text_and_rejects_binary_content() {
        let directory = tempfile::tempdir().unwrap();
        let text = directory.path().join("note.txt");
        let binary = directory.path().join("image.bin");
        std::fs::write(&text, "first\nsecond").unwrap();
        std::fs::write(&binary, [0_u8, 1, 2]).unwrap();

        assert_eq!(
            read_preview(&text),
            PreviewState::Text(vec!["first".to_string(), "second".to_string()])
        );
        assert_eq!(
            read_preview(&binary),
            PreviewState::Unavailable("Binary file".to_string())
        );
    }
}
