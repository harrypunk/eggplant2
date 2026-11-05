use ratatui::Frame;

use crate::data_object::Model;

#[derive(Debug, Default)]
pub struct Editor;

impl Editor {
    pub fn view(&self, _model: &Model, _frame: &mut Frame) {
        unimplemented!()
    }
}
