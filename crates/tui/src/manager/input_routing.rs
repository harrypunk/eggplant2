use crate::data_object::{Context, Model};

/// Resolves the input context at the application boundary.
///
/// Implementations compose page and feature policies; input consumers depend on
/// this interface rather than knowing those policies themselves.
pub trait InputRouting: std::fmt::Debug + Send + Sync {
    fn context(&self, model: &Model) -> Option<Context>;
}
