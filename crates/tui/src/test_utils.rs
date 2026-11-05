use std::path::Path;

use editor::DocumentFactory;

use crate::data_object::Model;

pub fn create_buffer(model: &mut Model, text: &str) {
    let document = DocumentFactory::new().create(text);
    model.editor.insert_document(document);
}

pub fn open_file(model: &mut Model, path: &Path) {
    let document = DocumentFactory::new().open(path).unwrap();
    model.editor.insert_document(document);
}
