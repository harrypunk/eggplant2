use editor::EditorSession;

use crate::{
    data_object::{AppPage, CommandId, Context, Model, PendingInput, StatusMessage},
    explorer::ExplorerState,
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
    navigation::VerticalNavigation,
    vcs::navigation::HistoryNavigation,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerticalDestination {
    Start,
    End,
    Index(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineJumpAction {
    Ignored,
    Pending,
    Cancelled,
    Complete(VerticalDestination),
}

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

    fn update(&mut self, character: char, accepts_count: bool) -> LineJumpAction {
        if !self.is_active() {
            return match character {
                '1'..='9' if accepts_count => {
                    self.push_digit(character as usize - '0' as usize);
                    LineJumpAction::Pending
                }
                'g' => {
                    self.pending_g = true;
                    LineJumpAction::Pending
                }
                'G' => self.finish(false),
                _ => LineJumpAction::Ignored,
            };
        }

        if self.pending_g {
            return match character {
                'g' => self.finish(true),
                _ => {
                    self.clear();
                    LineJumpAction::Cancelled
                }
            };
        }

        match character {
            '0'..='9' => {
                self.push_digit(character as usize - '0' as usize);
                LineJumpAction::Pending
            }
            'g' => {
                self.pending_g = true;
                LineJumpAction::Pending
            }
            'G' => self.finish(false),
            _ => {
                self.clear();
                LineJumpAction::Cancelled
            }
        }
    }

    fn finish(&mut self, first_line: bool) -> LineJumpAction {
        let destination = match (first_line, self.count) {
            (true, None) => VerticalDestination::Start,
            (false, None) => VerticalDestination::End,
            (_, Some(count)) => VerticalDestination::Index(count.saturating_sub(1)),
        };
        self.clear();
        LineJumpAction::Complete(destination)
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
    let accepts_count =
        !model.line_jump.is_active() && ('1'..='9').contains(&character) && accepts_count(model);
    match model.line_jump.update(character, accepts_count) {
        LineJumpAction::Ignored => {}
        LineJumpAction::Pending => activate(model),
        LineJumpAction::Cancelled => reset(model),
        LineJumpAction::Complete(destination) => complete(model, destination),
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
    if let Some(text) = model.line_jump.pending_text() {
        model.pending_input = Some(PendingInput::new(Context::LineJump, text));
    } else {
        PendingInput::clear(&mut model.pending_input, Context::LineJump);
    }
}

enum NavigationTarget<'a> {
    Editor(&'a mut EditorSession),
    Explorer(&'a mut ExplorerState),
    History(HistoryNavigation<'a>),
}

impl VerticalNavigation for NavigationTarget<'_> {
    fn item_count(&self) -> usize {
        match self {
            Self::Editor(target) => target.item_count(),
            Self::Explorer(target) => target.item_count(),
            Self::History(target) => target.item_count(),
        }
    }

    fn move_to(&mut self, index: usize) {
        match self {
            Self::Editor(target) => target.move_to(index),
            Self::Explorer(target) => target.move_to(index),
            Self::History(target) => target.move_to(index),
        }
    }

    fn accepts_count(&self) -> bool {
        match self {
            Self::Editor(target) => target.accepts_count(),
            Self::Explorer(target) => target.accepts_count(),
            Self::History(target) => target.accepts_count(),
        }
    }
}

fn navigation_target(model: &mut Model) -> NavigationTarget<'_> {
    match (model.active_page, model.context_stack.last()) {
        (_, Some(Context::Explorer)) => NavigationTarget::Explorer(&mut model.explorer),
        (AppPage::VcsHistory, _) => {
            NavigationTarget::History(HistoryNavigation::new(&mut model.vcs))
        }
        _ => NavigationTarget::Editor(&mut model.editor),
    }
}

fn accepts_count(model: &mut Model) -> bool {
    navigation_target(model).accepts_count()
}

fn complete(model: &mut Model, destination: VerticalDestination) {
    reset(model);
    navigate(&mut navigation_target(model), destination);
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
    PendingInput::clear(&mut model.pending_input, Context::LineJump);
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
    fn state_parses_counts_and_prefixes_without_a_model() {
        let mut state = LineJumpState::default();
        assert_eq!(state.update('2', false), LineJumpAction::Ignored);
        assert!(!state.is_active());

        assert_eq!(state.update('2', true), LineJumpAction::Pending);
        assert_eq!(state.pending_text().as_deref(), Some("2"));
        assert_eq!(state.update('g', true), LineJumpAction::Pending);
        assert_eq!(state.pending_text().as_deref(), Some("2g"));
        assert_eq!(
            state.update('g', true),
            LineJumpAction::Complete(VerticalDestination::Index(1))
        );
        assert!(!state.is_active());

        assert_eq!(state.update('g', true), LineJumpAction::Pending);
        assert_eq!(state.update('x', true), LineJumpAction::Cancelled);
        assert_eq!(
            state.update('G', true),
            LineJumpAction::Complete(VerticalDestination::End)
        );
    }

    #[test]
    fn counted_and_uncounted_jumps_show_pending_input() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "  éirst\nsecond\nthird\n\n\tfourth");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));

        input(&mut model, &manager, "g");
        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("g")
        );
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
        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("2")
        );
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
        model.vcs.history.state = crate::vcs::HistoryState::Ready(vec![
            commit("first"),
            commit("second"),
            commit("third"),
        ]);
        model.vcs.history.selected = Some(1);
        let manager = MessageManager::default();

        input(&mut model, &manager, "G");
        assert_eq!(model.vcs.history.selected, Some(2));
        input(&mut model, &manager, "gg");
        assert_eq!(model.vcs.history.selected, Some(0));
        input(&mut model, &manager, "2G");
        assert_eq!(model.vcs.history.selected, Some(2));

        let file = |path: &str| ::vcs::CommitFile {
            kind: ::vcs::ChangeKind::Modified,
            path: path.to_string(),
            previous_path: None,
        };
        model.vcs.history.detail = crate::vcs::CommitDetailsState::Ready(::vcs::CommitDetails {
            full_id: "third".to_string(),
            message: "message".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16 10:30:00 +0000".to_string(),
            files: vec![file("first.txt"), file("second.txt")],
        });
        model.vcs.history.pane = crate::vcs::HistoryPane::Detail;
        model.vcs.history.selected_file = Some(0);
        input(&mut model, &manager, "G");
        assert_eq!(model.vcs.history.selected_file, Some(1));
        input(&mut model, &manager, "2gg");
        assert_eq!(model.vcs.history.selected_file, Some(0));
        assert_eq!(model.pending_input, None);
    }
}
