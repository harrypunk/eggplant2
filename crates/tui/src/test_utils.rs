use std::path::Path;

use editor::{BufferId, DocumentFactory};

use crate::data_object::Model;

pub fn create_buffer(model: &mut Model, text: &str) -> BufferId {
    let document = DocumentFactory::new().create(text);
    model.editor.insert_document(document)
}

pub fn open_file(model: &mut Model, path: &Path) -> BufferId {
    let document = DocumentFactory::new().open(path).unwrap();
    model.editor.insert_document(document)
}
