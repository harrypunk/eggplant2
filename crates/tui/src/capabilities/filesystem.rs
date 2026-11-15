use std::{ffi::OsStr, path::Path};

use ignore::WalkBuilder;

const BUILT_IN_IGNORED_NAMES: &[&str] = &[".git", "node_modules", "__pycache__"];

/// Shared ignore policy for explorer and picker filesystem traversal.
pub(crate) fn is_built_in_ignored(name: &OsStr) -> bool {
    BUILT_IN_IGNORED_NAMES
        .iter()
        .any(|ignored| name == OsStr::new(ignored))
}

/// Traverse the fixed workspace with the explorer/picker ignore policy.
pub(crate) fn workspace_walk(root: &Path) -> WalkBuilder {
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
    builder
}
