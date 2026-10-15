use crate::data_object::{EditMode, Message, Model, RunningState};

#[derive(Debug, Default)]
pub struct MessageManager;

impl MessageManager {
    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Message> {
        match msg {
            Message::Quit => {
                model.running_state = RunningState::Done;
            }
            Message::EnterInsertMode => {
                model.edit_mode = EditMode::Insert;
            }
            Message::ExitInsertMode => {
                model.edit_mode = EditMode::Normal;
            }
        }
        None
    }
}
