//! Shared services used by features without coupling their domain logic.

pub mod document;
pub mod file_watcher;
pub mod register;
pub mod search;

pub(crate) mod filesystem;
