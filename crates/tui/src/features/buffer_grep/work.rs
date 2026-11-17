use std::sync::Arc;

use editor::BufferTextSnapshot;

use super::{BufferGrepState, GrepMatch};
use crate::{
    data_object::{Message, Model},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    search::search_text,
};

pub struct BufferGrepRequest {
    source: Arc<BufferTextSnapshot>,
    query: String,
    query_revision: u64,
}

impl BufferGrepRequest {
    pub fn new(source: Arc<BufferTextSnapshot>, query: String, query_revision: u64) -> Self {
        Self {
            source,
            query,
            query_revision,
        }
    }

    pub fn perform(self) -> Message {
        let result = search_text(&self.source.text, &self.query);
        Message::FeatureCompletion(FeatureCompletion::new(BufferGrepCompletion {
            source: (self.source.buffer_id, self.source.revision),
            query_revision: self.query_revision,
            result,
        }))
    }
}

impl FeatureEffect for BufferGrepRequest {
    fn perform(self: Box<Self>) -> Message {
        BufferGrepRequest::perform(*self)
    }
}

struct BufferGrepCompletion {
    source: (editor::BufferId, i32),
    query_revision: u64,
    result: Result<Vec<GrepMatch>, String>,
}

impl BufferGrepCompletion {
    fn apply(
        self,
        state: &mut BufferGrepState,
        active_revision: Option<(editor::BufferId, i32)>,
    ) -> bool {
        state.complete(
            self.query_revision,
            self.source,
            active_revision,
            self.result,
        )
    }
}

impl Completion for BufferGrepCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let active_revision = model.editor.active_revision();
        if self.apply(&mut model.buffer_grep, active_revision) {
            super::refresh_preview(model);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use editor::{DocumentFactory, EditorSession};

    use super::*;
    use crate::buffer_grep::SearchStatus;

    #[test]
    fn completion_applies_with_explicit_active_buffer_identity() {
        let mut session = EditorSession::new();
        let buffer_id = session.insert_document(DocumentFactory::new().create(""));
        let identity = (buffer_id, 4);
        let mut state = BufferGrepState::default();
        state.open(BufferTextSnapshot {
            buffer_id,
            revision: 4,
            text: "hello".to_string(),
        });
        let request = state.insert('e').expect("typing should start a search");

        let stale = BufferGrepCompletion {
            source: identity,
            query_revision: request.query_revision,
            result: Ok(Vec::new()),
        };
        assert!(!stale.apply(&mut state, Some((buffer_id, 5))));
        assert_eq!(state.status, SearchStatus::Searching);

        let current = BufferGrepCompletion {
            source: identity,
            query_revision: request.query_revision,
            result: Ok(Vec::new()),
        };
        assert!(current.apply(&mut state, Some(identity)));
        assert_eq!(state.status, SearchStatus::Ready);
    }
}
