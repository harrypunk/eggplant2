use crate::data_object::{Context, EditMode, Message, Model, RunningState};

#[derive(Debug, Default)]
pub struct MessageManager;

impl MessageManager {
    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Message> {
        match msg {
            Message::Quit => {
                model.running_state = RunningState::Done;
            }
            Message::EnterInsertMode => {
                model.context_stack.push(Context::EditorInput);
                model.edit_mode = EditMode::Insert;
            }
            Message::ExitInsertMode => {
                model
                    .context_stack
                    .pop_if(|ctx| ctx == &Context::EditorInput);
                model.edit_mode = EditMode::Normal;
            }
            _ => {}
        }
        None
    }
}
