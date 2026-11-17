use std::collections::HashSet;

use editor::BufferId;

#[derive(Debug, Default)]
pub(crate) struct BufferDiffState {
    loaded: HashSet<BufferId>,
    pending: Option<(BufferId, i32)>,
    generation: u64,
}

pub(crate) enum BufferDiffApply {
    Ignored,
    Apply {
        buffer_id: BufferId,
        revision: i32,
        base: Vec<u8>,
    },
}

impl BufferDiffState {
    pub(crate) fn needs_load(&self, buffer_id: BufferId, revision: i32) -> bool {
        !self.loaded.contains(&buffer_id) && self.pending != Some((buffer_id, revision))
    }

    pub(crate) fn begin(&mut self, buffer_id: BufferId, revision: i32) -> u64 {
        self.pending = Some((buffer_id, revision));
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    pub(crate) fn invalidate_all(&mut self) {
        self.loaded.clear();
        self.pending = None;
    }

    pub(crate) fn invalidate(&mut self, buffer_id: BufferId) {
        self.loaded.remove(&buffer_id);
        if self.pending.is_some_and(|(id, _)| id == buffer_id) {
            self.pending = None;
        }
    }

    pub(crate) fn complete(
        &mut self,
        buffer_id: BufferId,
        revision: i32,
        generation: u64,
        result: Result<Option<Vec<u8>>, String>,
    ) -> BufferDiffApply {
        if self.pending != Some((buffer_id, revision)) || self.generation != generation {
            return BufferDiffApply::Ignored;
        }
        self.pending = None;
        let Ok(base) = result else {
            return BufferDiffApply::Ignored;
        };
        BufferDiffApply::Apply {
            buffer_id,
            revision,
            base: base.unwrap_or_default(),
        }
    }

    pub(crate) fn mark_loaded(&mut self, buffer_id: BufferId) {
        self.loaded.insert(buffer_id);
    }

    #[cfg(test)]
    pub(crate) fn is_loaded(&self, buffer_id: BufferId) -> bool {
        self.loaded.contains(&buffer_id)
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> Option<(BufferId, i32)> {
        self.pending
    }

    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use editor::{DocumentFactory, EditorSession};

    use super::*;

    #[test]
    fn pending_identity_and_generation_guard_completion() {
        let buffer_id = EditorSession::new().insert_document(DocumentFactory::new().create(""));
        let mut state = BufferDiffState::default();
        let generation = state.begin(buffer_id, 4);

        assert!(matches!(
            state.complete(buffer_id, 5, generation, Ok(None)),
            BufferDiffApply::Ignored
        ));
        assert!(matches!(
            state.complete(buffer_id, 4, generation, Ok(Some(b"base".to_vec()))),
            BufferDiffApply::Apply { .. }
        ));
        assert_eq!(state.pending, None);
    }
}
