use ratatui::layout::Rect;

use crate::{
    data_object::{FocusDirection, FocusTarget, Model, StatusMessage, Visibility},
    manager::effect::Effect,
};

/// A window participating in directional focus navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FocusRegion {
    target: FocusTarget,
    area: Rect,
}

/// Window geometry used by the focus controller, independent of feature state.
#[derive(Debug, Clone, Default)]
pub struct FocusLayout {
    regions: Vec<FocusRegion>,
}

impl FocusLayout {
    pub fn from_regions(regions: impl IntoIterator<Item = (FocusTarget, Rect)>) -> Self {
        Self {
            regions: regions
                .into_iter()
                .filter(|(_, area)| area.width > 0 && area.height > 0)
                .map(|(target, area)| FocusRegion { target, area })
                .collect(),
        }
    }

    fn target_in_direction(
        &self,
        current: FocusTarget,
        direction: FocusDirection,
    ) -> Option<FocusTarget> {
        let current = self
            .regions
            .iter()
            .find(|region| region.target == current)?;
        self.regions
            .iter()
            .filter(|candidate| candidate.target != current.target)
            .filter(|candidate| direction.matches(current.area, candidate.area))
            .min_by_key(|candidate| direction.distance(current.area, candidate.area))
            .map(|candidate| candidate.target)
    }
}

trait FocusGeometry {
    fn matches(self, current: Rect, candidate: Rect) -> bool;
    fn distance(self, current: Rect, candidate: Rect) -> u64;
}

impl FocusGeometry for FocusDirection {
    fn matches(self, current: Rect, candidate: Rect) -> bool {
        let (current_x, current_y) = center(current);
        let (candidate_x, candidate_y) = center(candidate);
        match self {
            FocusDirection::Left => candidate_x < current_x,
            FocusDirection::Right => candidate_x > current_x,
            FocusDirection::Up => candidate_y < current_y,
            FocusDirection::Down => candidate_y > current_y,
        }
    }

    fn distance(self, current: Rect, candidate: Rect) -> u64 {
        let (current_x, current_y) = center(current);
        let (candidate_x, candidate_y) = center(candidate);
        let horizontal = current_x.abs_diff(candidate_x);
        let vertical = current_y.abs_diff(candidate_y);
        match self {
            FocusDirection::Left | FocusDirection::Right => horizontal + vertical * 10,
            FocusDirection::Up | FocusDirection::Down => vertical + horizontal * 10,
        }
    }
}

fn center(area: Rect) -> (u64, u64) {
    (
        u64::from(area.x) + u64::from(area.width) / 2,
        u64::from(area.y) + u64::from(area.height) / 2,
    )
}

/// Narrow host interface used by the focus controller.
///
/// Implementations own feature state and side effects; navigation itself depends
/// only on current focus and window geometry.
pub(crate) trait FocusHost {
    fn current_focus(&self) -> FocusTarget;
    fn focus_layout(&self) -> FocusLayout;
    fn focus_to(&mut self, target: FocusTarget) -> Option<Effect>;
    fn explorer_visibility(&self) -> Visibility;
    fn set_explorer_visibility(&mut self, visibility: Visibility);
    fn has_workspace(&self) -> bool;
    fn set_status(&mut self, message: StatusMessage);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FocusManager;

impl FocusManager {
    pub(crate) fn move_focus(
        host: &mut impl FocusHost,
        direction: FocusDirection,
    ) -> Option<Effect> {
        let target = host
            .focus_layout()
            .target_in_direction(host.current_focus(), direction)?;
        host.focus_to(target)
    }

    pub(crate) fn toggle_explorer(host: &mut impl FocusHost) -> Option<Effect> {
        match host.explorer_visibility() {
            Visibility::Hidden if !host.has_workspace() => {
                host.set_status(StatusMessage::info("No folder open"));
            }
            Visibility::Hidden => {
                host.set_explorer_visibility(Visibility::Displayed);
                host.focus_to(FocusTarget::FileExplorer);
            }
            Visibility::Displayed => {
                host.set_explorer_visibility(Visibility::Hidden);
                if host.current_focus() == FocusTarget::FileExplorer {
                    host.focus_to(FocusTarget::Editor);
                }
            }
        }
        None
    }
}

pub fn toggle_explorer(model: &mut Model) -> Option<Effect> {
    FocusManager::toggle_explorer(model)
}

pub fn focus_left(model: &mut Model) -> Option<Effect> {
    FocusManager::move_focus(model, FocusDirection::Left)
}

pub fn focus_down(model: &mut Model) -> Option<Effect> {
    FocusManager::move_focus(model, FocusDirection::Down)
}

pub fn focus_up(model: &mut Model) -> Option<Effect> {
    FocusManager::move_focus(model, FocusDirection::Up)
}

pub fn focus_right(model: &mut Model) -> Option<Effect> {
    FocusManager::move_focus(model, FocusDirection::Right)
}

impl FocusHost for Model {
    fn current_focus(&self) -> FocusTarget {
        self.current_focus
    }

    fn focus_layout(&self) -> FocusLayout {
        let explorer = if self.workspace.is_some() {
            self.explorer.visibility
        } else {
            Visibility::Hidden
        };
        crate::page::editor::focus_layout(self.window_area, explorer, self.tools.visibility)
    }

    fn focus_to(&mut self, target: FocusTarget) -> Option<Effect> {
        match target {
            FocusTarget::FileExplorer => {
                focus_explorer(self);
                None
            }
            FocusTarget::Editor => {
                if self.current_focus == FocusTarget::ToolsPanel
                    && self.tools.active_tool == Some(crate::tools::ToolKind::Agent)
                {
                    crate::agent::close_panel_input(self);
                } else {
                    focus_editor(self);
                }
                None
            }
            FocusTarget::ToolsPanel => crate::agent::focus_panel_input(self),
            FocusTarget::FilePicker | FocusTarget::AgentChatDialog => None,
        }
    }

    fn explorer_visibility(&self) -> Visibility {
        self.explorer.visibility
    }

    fn set_explorer_visibility(&mut self, visibility: Visibility) {
        self.explorer.visibility = visibility;
    }

    fn has_workspace(&self) -> bool {
        self.workspace.is_some()
    }

    fn set_status(&mut self, message: StatusMessage) {
        self.status = Some(message);
    }
}

fn focus_explorer(model: &mut Model) {
    model.current_focus = FocusTarget::FileExplorer;
}

pub(crate) fn focus_editor(model: &mut Model) {
    model.current_focus = FocusTarget::Editor;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{CommandId, Context, Message},
        manager::message_manager::MessageManager,
        startup::initial_model,
        test_utils::create_buffer,
    };

    #[test]
    fn directional_layout_uses_window_centers_and_nearest_target() {
        let layout = FocusLayout::from_regions([
            (FocusTarget::FileExplorer, Rect::new(0, 0, 10, 10)),
            (FocusTarget::Editor, Rect::new(12, 0, 30, 10)),
            (FocusTarget::ToolsPanel, Rect::new(44, 0, 10, 10)),
        ]);
        assert_eq!(
            layout.target_in_direction(FocusTarget::Editor, FocusDirection::Left),
            Some(FocusTarget::FileExplorer)
        );
        assert_eq!(
            layout.target_in_direction(FocusTarget::Editor, FocusDirection::Right),
            Some(FocusTarget::ToolsPanel)
        );
        assert_eq!(
            layout.target_in_direction(FocusTarget::ToolsPanel, FocusDirection::Left),
            Some(FocusTarget::Editor)
        );
    }

    #[test]
    fn hiding_and_showing_explorer_updates_focus() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.txt"), "a").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let original_entries = model.explorer.entries.clone();
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        assert_eq!(model.explorer.visibility, Visibility::Hidden);
        assert_eq!(model.explorer.entries, original_entries);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert!(model.context_stack.is_empty());

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        assert_eq!(model.explorer.visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn directional_focus_moves_between_editor_and_explorer() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        create_buffer(&mut model, "buffer");
        let manager = MessageManager::default();

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_right")));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert!(model.context_stack.is_empty());

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_left")));
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert!(model.context_stack.is_empty());

        for command in ["focus_up", "focus_down"] {
            manager.update(&mut model, Message::InvokeCommand(CommandId(command)));
            assert_eq!(model.current_focus, FocusTarget::FileExplorer);
            assert!(model.context_stack.is_empty());
        }
    }

    #[test]
    fn horizontal_focus_reuses_existing_keys_for_agent_panel() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        crate::test_utils::open_file(&mut model, &path);
        model.current_focus = FocusTarget::Editor;
        model.tools.active_tool = Some(crate::tools::ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;

        focus_right(&mut model);
        assert_eq!(model.current_focus, FocusTarget::ToolsPanel);
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::AgentChat)
        );

        focus_left(&mut model);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_ne!(
            crate::test_utils::input_context(&model),
            Some(Context::AgentChat)
        );
    }

    #[test]
    fn agent_panel_focus_does_not_require_an_open_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.tools.active_tool = Some(crate::tools::ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        model.current_focus = FocusTarget::Editor;

        focus_right(&mut model);
        assert_eq!(model.current_focus, FocusTarget::ToolsPanel);
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::AgentChat)
        );
        focus_left(&mut model);
        assert_eq!(model.current_focus, FocusTarget::Editor);
    }

    #[test]
    fn leaving_diagnostics_does_not_reset_agent_state() {
        let mut model = Model {
            current_focus: FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(crate::tools::ToolKind::Diagnostics);
        model.tools.visibility = Visibility::Displayed;
        model.agent.enter_insert_mode();
        model
            .agent
            .handle_input_event(crate::component::text_input::TextInputEvent::Insert('x'));

        focus_left(&mut model);

        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.agent.prompt(), "x");
        assert_eq!(
            model.agent.panel_mode(),
            crate::agent::AgentInputMode::Insert
        );
    }

    #[test]
    fn directional_focus_ignores_unavailable_targets() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_right")));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert!(model.context_stack.is_empty());

        create_buffer(&mut model, "buffer");
        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_right")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_left")));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert!(model.context_stack.is_empty());

        let mut without_workspace = Model::default();
        create_buffer(&mut without_workspace, "buffer");
        manager.update(
            &mut without_workspace,
            Message::InvokeCommand(CommandId("focus_left")),
        );
        assert_eq!(without_workspace.current_focus, FocusTarget::Editor);
        assert!(without_workspace.context_stack.is_empty());
    }
}
