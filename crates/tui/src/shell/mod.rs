//! Application-shell coordination for buffers, menus, focus, and lifecycle.

pub mod buffer;
pub mod command_menu;
pub mod focus;
pub mod lifecycle;

use crate::{data_object::Context, manager::keymap_contribution::ContextKeymap};

/// The persistent editor surface; its direct defaults come from `Config::default()`.
pub const KEYMAPS: &[ContextKeymap] = &[ContextKeymap::new("normal", Context::AppNormal)];
