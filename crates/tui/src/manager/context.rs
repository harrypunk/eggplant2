use crate::data_object::{Context, Message};
use crossterm::event::KeyCode;
use std::collections::HashMap;

type KeyCommandMap = HashMap<KeyCode, Message>;
/// A context and related keybindings
/// Example:
/// context EditorInput to input text in editor
/// context AppNormal with keys for normal app functionality
/// context AgentChat to chat with AI in input fields
#[derive(Clone)]
pub struct ContextBindings {
    key_command_map: KeyCommandMap,
    pub context: Context,
}

impl ContextBindings {
    pub fn new(context: Context) -> Self {
        Self {
            key_command_map: HashMap::new(),
            context,
        }
    }

    pub fn new_with_mapping(context: Context, mapping: KeyCommandMap) -> Self {
        Self {
            key_command_map: mapping,
            context,
        }
    }

    pub fn register(&mut self, key_code: KeyCode, msg: Message) {
        self.key_command_map.insert(key_code, msg);
    }

    pub fn resolve(&self, key_code: KeyCode) -> Option<Message> {
        self.key_command_map.get(&key_code).cloned()
    }
}
