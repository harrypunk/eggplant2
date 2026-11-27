//! Workspace file discovery and fuzzy matching shared by file-oriented workflows.

use std::path::{Path, PathBuf};

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

use super::filesystem::workspace_walk;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDepth {
    TopLevel,
    Recursive,
}

/// Scan workspace files with the same ignore policy used by the FilePicker.
pub(crate) fn scan_files(root: &Path, depth: ScanDepth) -> Result<Vec<FileEntry>, String> {
    let mut builder = workspace_walk(root);
    builder.max_depth(match depth {
        ScanDepth::TopLevel => Some(1),
        ScanDepth::Recursive => None,
    });

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

/// Fuzzy-filter and deterministically rank existing workspace entries.
pub(crate) fn filter_files(files: &[FileEntry], query: &str) -> Vec<FileEntry> {
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
