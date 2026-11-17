//! Shared services used by features without coupling their domain logic.

pub mod clipboard;
pub mod copy_hash;
pub mod document;
pub mod file_watcher;
pub mod notification;
pub mod page_scroll;
pub mod register;
pub mod search;

pub(crate) mod filesystem;
