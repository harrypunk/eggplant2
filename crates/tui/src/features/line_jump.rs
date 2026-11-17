use crate::{
    data_object::{AppPage, CommandId, Context, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
    navigation::VerticalNavigation,
    vcs::navigation::HistoryNavigation,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LineJumpState {
    count: Option<usize>,
    pending_g: bool,
}

impl LineJumpState {
    fn is_active(self) -> bool {
        self.count.is_some() || self.pending_g
    }

    fn pending_text(self) -> Option<String> {
        match (self.count, self.pending_g) {
            (Some(count), true) => Some(format!("{count}g")),
            (Some(count), false) => Some(count.to_string()),
            (None, true) => Some("g".to_string()),
            (None, false) => None,
        }
    }

    fn push_digit(&mut self, digit: usize) {
        self.count = Some(
            self.count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit),
        );
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    commands.register(Command::feature(
        CommandId("line_jump_cancel"),
        "cancel pending line jump",
        handle_command,
    ))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::AppNormal, handle_input)?;
    input.register(Context::LineJump, handle_input)?;
    input.register(Context::Explorer, handle_input)?;
    input.register(Context::VcsHistory, handle_input)?;
    input.register(Context::EditorVisual, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "line_jump_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown line-jump command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

/// Consume only count and gg/G input; unrelated printable input remains a no-op.
pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    if !model.line_jump.is_active() {
        match character {
            '1'..='9' if accepts_count(model) => {
                model
                    .line_jump
                    .push_digit(character as usize - '0' as usize);
                activate(model);
            }
            'g' => {
                model.line_jump.pending_g = true;
                activate(model);
            }
            'G' => complete(model, false),
            _ => {}
        }
        return None;
    }

    if model.line_jump.pending_g {
        match character {
            'g' => complete(model, true),
            _ => reset(model),
        }
        return None;
    }

    match character {
        '0'..='9' => {
            model
                .line_jump
                .push_digit(character as usize - '0' as usize);
            update_pending(model);
        }
        'g' => {
            model.line_jump.pending_g = true;
            update_pending(model);
        }
        'G' => complete(model, false),
        _ => reset(model),
    }
    None
}

fn activate(model: &mut Model) {
    if model.context_stack.last() != Some(&Context::LineJump) {
        model.context_stack.push(Context::LineJump);
    }
    update_pending(model);
}

fn update_pending(model: &mut Model) {
    model.pending_input = model.line_jump.pending_text();
}

#[derive(Clone, Copy)]
enum VerticalDestination {
    Start,
    End,
    Index(usize),
}

fn accepts_count(model: &mut Model) -> bool {
    match (model.active_page, model.context_stack.last()) {
        (_, Some(Context::Explorer)) => model.explorer.accepts_count(),
        (AppPage::VcsHistory, _) => HistoryNavigation::new(&mut model.vcs).accepts_count(),
        _ => model.editor.accepts_count(),
    }
}

fn complete(model: &mut Model, first_line: bool) {
    let destination = match (first_line, model.line_jump.count) {
        (true, None) => VerticalDestination::Start,
        (false, None) => VerticalDestination::End,
        (_, Some(count)) => VerticalDestination::Index(count.saturating_sub(1)),
    };
    reset(model);
    match (model.active_page, model.context_stack.last()) {
        (_, Some(Context::Explorer)) => navigate(&mut model.explorer, destination),
        (AppPage::VcsHistory, _) => {
            navigate(&mut HistoryNavigation::new(&mut model.vcs), destination)
        }
        _ => navigate(&mut model.editor, destination),
    }
}

fn navigate(target: &mut impl VerticalNavigation, destination: VerticalDestination) {
    match destination {
        VerticalDestination::Start => target.move_to_start(),
        VerticalDestination::End => target.move_to_end(),
        VerticalDestination::Index(index) => target.move_to(index),
    }
}

fn reset(model: &mut Model) {
    model.line_jump.clear();
    model.pending_input = None;
    model
        .context_stack
        .pop_if(|context| context == &Context::LineJump);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    fn input(model: &mut Model, manager: &MessageManager, text: &str) {
        for character in text.chars() {
            manager.update(model, Message::TextInput(character));
        }
    }

    fn cursor_line(model: &Model) -> usize {
        let snapshot = model.editor.active_snapshot().unwrap();
        snapshot.text.first_line + snapshot.cursor.unwrap().row
    }

    #[test]
    fn counted_and_uncounted_jumps_show_pending_input() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "  éirst\nsecond\nthird\n\n\tfourth");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));

        input(&mut model, &manager, "g");
        assert_eq!(model.pending_input.as_deref(), Some("g"));
        assert_eq!(model.context_stack.last(), Some(&Context::LineJump));
        input(&mut model, &manager, "g");
        assert_eq!(cursor_line(&model), 0);
        assert_eq!(model.pending_input, None);

        input(&mut model, &manager, "3g");
        input(&mut model, &manager, "g");
        assert_eq!(cursor_line(&model), 2);
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            0
        );

        input(&mut model, &manager, "2");
        assert_eq!(model.pending_input.as_deref(), Some("2"));
        input(&mut model, &manager, "G");
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.cursor.unwrap().column, 0);

        input(&mut model, &manager, "5G");
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            4
        );
        input(&mut model, &manager, "99G");
        assert_eq!(cursor_line(&model), 4);

        input(&mut model, &manager, "G");
        assert_eq!(cursor_line(&model), 4);
    }

    #[test]
    fn empty_buffer_clamps_large_counts_without_changes() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));

        input(&mut model, &manager, "999999999999999999999999G");

        assert_eq!(cursor_line(&model), 0);
        assert_eq!(model.pending_input, None);
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn escape_invalid_input_and_zero_do_not_move() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));

        input(&mut model, &manager, "0");
        assert_eq!(model.pending_input, None);
        input(&mut model, &manager, "2");
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("line_jump_cancel")),
        );
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(cursor_line(&model), 0);

        input(&mut model, &manager, "2g");
        input(&mut model, &manager, "x");
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(cursor_line(&model), 0);
    }

    #[test]
    fn explorer_supports_gg_and_g_but_ignores_counts() {
        let mut model = Model::default();
        model.context_stack.push(Context::Explorer);
        model.explorer.entries = (0..3)
            .map(|index| crate::explorer::ExplorerEntry {
                name: format!("file-{index}").into(),
                path: std::path::PathBuf::from(format!("/workspace/file-{index}")),
                kind: crate::explorer::ExplorerEntryKind::File,
                depth: 0,
                expansion: crate::explorer::ExpansionState::Collapsed,
            })
            .collect();
        model.explorer.selected = Some(1);
        let manager = MessageManager::default();

        input(&mut model, &manager, "G");
        assert_eq!(model.explorer.selected, Some(2));
        input(&mut model, &manager, "gg");
        assert_eq!(model.explorer.selected, Some(0));
        input(&mut model, &manager, "2G");
        assert_eq!(model.explorer.selected, Some(2));
        assert_eq!(model.pending_input, None);
    }

    #[test]
    fn history_and_detail_support_gg_and_g_but_ignore_counts() {
        let commit = |id: &str| ::vcs::CommitSummary {
            id: id.to_string(),
            subject: id.to_string(),
            author: "Eggplant".to_string(),
            timestamp: 0,
            branches: Vec::new(),
        };
        let mut model = Model {
            active_page: AppPage::VcsHistory,
            ..Model::default()
        };
        model.context_stack.push(Context::VcsHistory);
        model.vcs.history = crate::vcs::HistoryState::Ready(vec![
            commit("first"),
            commit("second"),
            commit("third"),
        ]);
        model.vcs.history_selected = Some(1);
        let manager = MessageManager::default();

        input(&mut model, &manager, "G");
        assert_eq!(model.vcs.history_selected, Some(2));
        input(&mut model, &manager, "gg");
        assert_eq!(model.vcs.history_selected, Some(0));
        input(&mut model, &manager, "2G");
        assert_eq!(model.vcs.history_selected, Some(2));

        let file = |path: &str| ::vcs::CommitFile {
            kind: ::vcs::ChangeKind::Modified,
            path: path.to_string(),
            previous_path: None,
        };
        model.vcs.history_detail = crate::vcs::CommitDetailsState::Ready(::vcs::CommitDetails {
            full_id: "third".to_string(),
            message: "message".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16 10:30:00 +0000".to_string(),
            files: vec![file("first.txt"), file("second.txt")],
        });
        model.vcs.history_detail_selected_file = Some(0);
        input(&mut model, &manager, "G");
        assert_eq!(model.vcs.history_detail_selected_file, Some(1));
        input(&mut model, &manager, "2gg");
        assert_eq!(model.vcs.history_detail_selected_file, Some(0));
        assert_eq!(model.pending_input, None);
    }
}
