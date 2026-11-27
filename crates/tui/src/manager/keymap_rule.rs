use crate::manager::context::{InputAction, KeyChord};

/// Reusable key-resolution behavior composed after direct configured bindings.
pub trait KeymapRule: std::fmt::Debug + Send + Sync {
    fn resolve(&self, key: KeyChord) -> Option<InputAction>;
}
