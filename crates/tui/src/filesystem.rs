use std::ffi::OsStr;

const BUILT_IN_IGNORED_NAMES: &[&str] = &[".git", "node_modules", "__pycache__"];

/// Shared ignore policy for explorer and picker filesystem traversal.
pub(crate) fn is_built_in_ignored(name: &OsStr) -> bool {
    BUILT_IN_IGNORED_NAMES
        .iter()
        .any(|ignored| name == OsStr::new(ignored))
}
