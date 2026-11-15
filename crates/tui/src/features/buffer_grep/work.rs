use std::sync::Arc;

use editor::BufferTextSnapshot;

use super::GrepMatch;
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

impl Completion for BufferGrepCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let active_revision = model.editor.active_revision();
        if model.buffer_grep.complete(
            self.query_revision,
            self.source,
            active_revision,
            self.result,
        ) {
            super::refresh_preview(model);
        }
        None
    }
}
