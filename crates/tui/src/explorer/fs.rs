use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
};

use ignore::WalkBuilder;

use super::state::{ExpansionState, ExplorerEntry, ExplorerEntryKind, IgnoreMode};
use crate::filesystem::is_built_in_ignored;

pub(super) fn read_visible_entries(
    root: &Path,
    expanded_directories: &HashSet<PathBuf>,
    ignore_mode: IgnoreMode,
) -> io::Result<Vec<ExplorerEntry>> {
    read_directory(root, 0, expanded_directories, ignore_mode)
}

/// Read and sort one directory level without recursively expanding children.
fn read_entries(directory: &Path, ignore_mode: IgnoreMode) -> io::Result<Vec<ExplorerEntry>> {
    let mut builder = WalkBuilder::new(directory);
    builder
        .max_depth(Some(1))
        // Git-ignore filtering should not implicitly hide unrelated dotfiles.
        .hidden(false)
        .parents(true)
        .ignore(false)
        .git_ignore(ignore_mode == IgnoreMode::HideIgnored)
        .git_global(false)
        .git_exclude(false)
        .require_git(false);

    let mut entries = builder
        .build()
        .filter_map(|entry| match entry {
            // Keep the root in the walk so its ignore files are loaded, but do
            // not display the workspace itself as one of its children.
            Ok(entry) if entry.depth() == 0 => None,
            Ok(entry)
                if ignore_mode == IgnoreMode::HideIgnored
                    && is_built_in_ignored(entry.file_name()) =>
            {
                None
            }
            entry => Some(entry),
        })
        .map(read_entry)
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort_by(|left, right| {
        left.kind
            .sort_rank()
            .cmp(&right.kind.sort_rank())
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(entries)
}

fn read_entry(entry: Result<ignore::DirEntry, ignore::Error>) -> io::Result<ExplorerEntry> {
    let entry = entry.map_err(io::Error::other)?;
    Ok(ExplorerEntry {
        name: entry.file_name().to_os_string(),
        kind: entry_kind(&entry),
        path: entry.path().to_path_buf(),
        depth: 0,
        expansion: ExpansionState::Collapsed,
    })
}

fn entry_kind(entry: &ignore::DirEntry) -> ExplorerEntryKind {
    let Some(file_type) = entry.file_type() else {
        return ExplorerEntryKind::Other;
    };
    // Never follow directory links during tree expansion; links are shown but
    // are not valid document-open targets.
    if file_type.is_symlink() {
        ExplorerEntryKind::Link
    } else if file_type.is_dir() {
        ExplorerEntryKind::Directory
    } else if file_type.is_file() {
        ExplorerEntryKind::File
    } else {
        ExplorerEntryKind::Other
    }
}

/// Flatten one directory level and recurse only into expanded directories.
fn read_directory(
    directory: &Path,
    depth: usize,
    expanded_directories: &HashSet<PathBuf>,
    ignore_mode: IgnoreMode,
) -> io::Result<Vec<ExplorerEntry>> {
    read_entries(directory, ignore_mode)?.into_iter().try_fold(
        Vec::new(),
        |mut visible, mut entry| {
            entry.depth = depth;
            entry.expansion = match entry.kind {
                ExplorerEntryKind::Directory if expanded_directories.contains(&entry.path) => {
                    ExpansionState::Expanded
                }
                _ => ExpansionState::Collapsed,
            };
            let path = entry.path.clone();
            let expanded = entry.expansion == ExpansionState::Expanded;
            visible.push(entry);

            if expanded {
                visible.extend(read_directory(
                    &path,
                    depth + 1,
                    expanded_directories,
                    ignore_mode,
                )?);
            }
            Ok(visible)
        },
    )
}
