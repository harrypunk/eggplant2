use std::{
    collections::HashSet,
    fs::DirEntry,
    io,
    path::{Path, PathBuf},
};

use super::state::{ExplorerEntry, ExplorerEntryKind};

pub(super) fn read_visible_entries(
    root: &Path,
    expanded_directories: &HashSet<PathBuf>,
) -> io::Result<Vec<ExplorerEntry>> {
    read_directory(root, 0, expanded_directories)
}

/// Read and sort one directory level without recursively expanding children.
fn read_entries(directory: &Path) -> io::Result<Vec<ExplorerEntry>> {
    let mut entries = std::fs::read_dir(directory)?
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

fn read_entry(entry: io::Result<DirEntry>) -> io::Result<ExplorerEntry> {
    let entry = entry?;
    let path = entry.path();
    Ok(ExplorerEntry {
        name: entry.file_name(),
        kind: entry_kind(&entry)?,
        path,
        depth: 0,
        expanded: false,
    })
}

fn entry_kind(entry: &DirEntry) -> io::Result<ExplorerEntryKind> {
    let file_type = entry.file_type()?;
    // Follow symlinks to files, but not directories: expanding a symlinked
    // directory could introduce a filesystem cycle.
    let kind = if file_type.is_dir() {
        ExplorerEntryKind::Directory
    } else if file_type.is_file() || entry.path().is_file() {
        ExplorerEntryKind::File
    } else {
        ExplorerEntryKind::Other
    };
    Ok(kind)
}

/// Flatten one directory level and recurse only into expanded directories.
fn read_directory(
    directory: &Path,
    depth: usize,
    expanded_directories: &HashSet<PathBuf>,
) -> io::Result<Vec<ExplorerEntry>> {
    read_entries(directory)?
        .into_iter()
        .try_fold(Vec::new(), |mut visible, mut entry| {
            entry.depth = depth;
            entry.expanded = entry.kind == ExplorerEntryKind::Directory
                && expanded_directories.contains(&entry.path);
            let path = entry.path.clone();
            let expanded = entry.expanded;
            visible.push(entry);

            if expanded {
                visible.extend(read_directory(&path, depth + 1, expanded_directories)?);
            }
            Ok(visible)
        })
}
