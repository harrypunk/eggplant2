//! Provider-neutral agent persistence records, storage ports, and in-memory store.
//! Database drivers and frontend state remain outside this crate.

mod memory;
mod provider;
mod records;
mod store;

pub use memory::InMemoryStore;
pub use provider::*;
pub use records::*;
pub use store::{SessionStore, StorageError, StoreFuture};

#[cfg(test)]
mod tests;
