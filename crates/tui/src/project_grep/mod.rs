mod component;
mod state;
mod work;

pub use component::ProjectGrep;
pub use state::{ProjectGrepResult, ProjectGrepState, SearchStatus};
pub use work::ProjectGrepRequest;

use editor::DocumentPosition;

use crate::{
    command_menu::CommandMenuItem,
    data_object::{CommandId, Context, Model, StatusMessage},
    document,
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
    ui::UiRegistry,
};

pub static PROJECT_MENU_ITEM: CommandMenuItem =
    CommandMenuItem::action('p', "project", CommandId("project_grep_open"));

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("project_grep_open"),
            "search project",
            handle_command,
        ),
        Command::feature(
            CommandId("project_grep_close"),
            "close project search",
            handle_command,
        ),
        Command::feature(
            CommandId("project_grep_backspace"),
            "delete project search character",
            handle_command,
        ),
        Command::feature(
            CommandId("project_grep_next"),
            "next project search match",
            handle_command,
        ),
        Command::feature(
            CommandId("project_grep_previous"),
            "previous project search match",
            handle_command,
        ),
        Command::feature(
            CommandId("project_grep_accept"),
            "open project search match",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::ProjectGrep, handle_input)
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("project_grep", component::overlay)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "project_grep_open" => open(model),
        "project_grep_close" => close(model),
        "project_grep_backspace" => {
            return model.project_grep.backspace().map(Effect::feature);
        }
        "project_grep_next" => {
            return model.project_grep.select_next().map(Effect::feature);
        }
        "project_grep_previous" => {
            return model.project_grep.select_previous().map(Effect::feature);
        }
        "project_grep_accept" => return accept(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown project-grep command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    model.project_grep.insert(character).map(Effect::feature)
}

fn open(model: &mut Model) {
    // Only Normal or Explorer may open grep; another modal keeps its input context.
    let Some(Context::AppNormal | Context::Explorer) = model.context_stack.last() else {
        return;
    };
    let Some(root) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
    else {
        model.status = Some(StatusMessage::info("No folder open"));
        return;
    };
    model.status = None;
    model.project_grep.open(root);
    model.context_stack.push(Context::ProjectGrep);
}

fn close(model: &mut Model) {
    model.project_grep.close();
    model
        .context_stack
        .pop_if(|context| context == &Context::ProjectGrep);
}

fn accept(model: &mut Model) -> Option<Effect> {
    let result = model.project_grep.selected_result()?;
    let path = result.path.clone();
    let position = DocumentPosition {
        line: result.line,
        character: 0,
    };
    let documents = model.project_grep.documents().clone();
    close(model);
    document::open_file_at(&documents, model, &path, Some(position))
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, startup::initial_model,
    };

    #[test]
    fn leader_live_search_preview_and_acceptance_open_the_match_line() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(
            &path,
            "fn main() {}\nprintln!(\"hello\");\nprintln!(\"HELLO\");",
        )
        .unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(100, 20));

        for key in [' ', 's', 'p'] {
            manager.update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)));
        }
        assert_eq!(model.context_stack.last(), Some(&Context::ProjectGrep));

        let mut effect = None;
        for character in "ell".chars() {
            effect = manager.update(&mut model, Message::TextInput(character));
        }
        let Some(Effect::Feature(request)) = effect else {
            panic!("a non-empty query should start a project search");
        };
        let Some(Effect::Feature(preview)) = manager.update(&mut model, request.perform()) else {
            panic!("a completed search should start a preview");
        };
        manager.update(&mut model, preview.perform());
        assert_eq!(model.project_grep.results.len(), 2);
        assert!(model.project_grep.preview.is_some());

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("project_grep_next")),
        );
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("project_grep_accept")),
        );
        let Some(Effect::OpenDocument(request)) = effect else {
            panic!("acceptance should open the selected file");
        };
        manager.update(&mut model, Message::FileOpened(request.perform()));

        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.cursor.unwrap().row, 2);
        assert_eq!(snapshot.cursor.unwrap().column, 0);
        assert_eq!(
            model.project_grep.visibility,
            crate::data_object::Visibility::Hidden
        );
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn project_search_requires_a_workspace() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("project_grep_open")),
        );

        assert_eq!(
            model.project_grep.visibility,
            crate::data_object::Visibility::Hidden
        );
        assert_eq!(model.status, Some(StatusMessage::info("No folder open")));
    }
}
