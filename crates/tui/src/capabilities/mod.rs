//! Shared services used by features without coupling their domain logic.

pub mod clipboard;
pub mod copy_hash;
pub mod document;
pub mod file_watcher;
pub use app::language;
pub mod notification;
pub mod openai;
pub mod page_scroll;
pub mod paste;
pub mod readonly;
pub mod register;
pub mod search;

pub(crate) mod file_search;
pub(crate) mod filesystem;
