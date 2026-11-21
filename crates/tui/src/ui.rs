use std::collections::HashMap;

use color_eyre::eyre::eyre;
use editor::EditorSnapshot;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Position, Rect, Size},
};

use crate::{
    component::{
        diagnostics_panel::DiagnosticsPanel, file_explorer::FileExplorer, key_hints::KeyHints,
        status_bar::StatusBar, tools_panel::ToolsPanel,
    },
    data_object::{FocusState, FocusTarget, Model, PendingInput},
};

pub type RenderContribution = fn(&Model, &mut Frame, Rect);
pub type OverlayCursor = fn(&Model, Rect) -> Option<Position>;
pub type OverlayProvider = fn(&Model) -> Option<OverlayContribution>;
pub type EditorOverlayRender = fn(&Model, &EditorSnapshot, Rect, &mut Buffer);
pub type EditorOverlayProvider = fn(&Model) -> Option<EditorOverlayContribution>;

#[derive(Debug, Clone, Copy)]
pub struct EditorOverlayContribution {
    pub id: &'static str,
    pub render: EditorOverlayRender,
}

impl EditorOverlayContribution {
    pub fn new(id: &'static str, render: EditorOverlayRender) -> Self {
        Self { id, render }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelSlot {
    Explorer,
    Tools,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusSlot {
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OverlayKind {
    Hint,
    Modal,
}

#[derive(Debug, Clone, Copy)]
pub enum OverlayPlacement {
    Centered(Size),
    BottomRight(Size),
    MainPanel {
        max_width: u16,
        max_height: u16,
        margin: u16,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct OverlayContribution {
    pub id: &'static str,
    pub kind: OverlayKind,
    pub placement: OverlayPlacement,
    pub render: RenderContribution,
    pub cursor: Option<OverlayCursor>,
}

impl OverlayContribution {
    pub fn new(
        id: &'static str,
        kind: OverlayKind,
        placement: OverlayPlacement,
        render: RenderContribution,
    ) -> Self {
        Self {
            id,
            kind,
            placement,
            render,
            cursor: None,
        }
    }

    pub fn with_cursor(mut self, cursor: OverlayCursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn area(self, main: Rect, frame: Rect) -> Rect {
        match self.placement {
            OverlayPlacement::Centered(size) => centered_area(main, size),
            OverlayPlacement::BottomRight(size) => bottom_right_area(frame, size),
            OverlayPlacement::MainPanel {
                max_width,
                max_height,
                margin,
            } => main_panel_area(main, max_width, max_height, margin),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct UiRegistry {
    panels: HashMap<PanelSlot, RenderContribution>,
    status: HashMap<StatusSlot, RenderContribution>,
    overlays: Vec<(&'static str, OverlayProvider)>,
    editor_overlays: Vec<(&'static str, EditorOverlayProvider)>,
}

impl UiRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn builtin() -> Self {
        let mut ui = Self::empty();
        ui.register_panel(PanelSlot::Explorer, render_explorer_panel)
            .expect("explorer panel must be unique");
        ui.register_panel(PanelSlot::Tools, render_tools_panel)
            .expect("tools panel must be unique");
        ui.register_status(StatusSlot::Message, render_status)
            .expect("status slot must be unique");
        ui.register_overlay("explorer_help", crate::explorer::help_overlay)
            .expect("explorer help overlay must be unique");
        ui.register_overlay("explorer_add", crate::explorer::add_overlay)
            .expect("explorer add overlay must be unique");
        ui.register_overlay("explorer_delete", crate::explorer::delete_overlay)
            .expect("explorer delete overlay must be unique");
        ui.register_overlay("explorer_transfer", crate::explorer::transfer_overlay)
            .expect("explorer transfer overlay must be unique");
        ui.register_overlay("code_navigation", crate::code_navigation::overlay)
            .expect("code-navigation overlay must be unique");
        ui.register_overlay("file_picker", crate::component::file_picker::overlay)
            .expect("file picker overlay must be unique");
        ui.register_overlay("theme_picker", crate::theme_picker::overlay)
            .expect("theme picker overlay must be unique");
        ui.register_overlay("command_menu", command_menu_overlay)
            .expect("command menu overlay must be unique");
        crate::buffer_jump::register_ui(&mut ui).expect("buffer-jump UI must be unique");
        crate::buffer_grep::register_ui(&mut ui).expect("buffer-grep UI must be unique");
        crate::project_grep::register_ui(&mut ui).expect("project-grep UI must be unique");
        crate::agent::register_ui(&mut ui).expect("agent UI must be unique");
        ui
    }

    pub fn register_panel(
        &mut self,
        slot: PanelSlot,
        render: RenderContribution,
    ) -> color_eyre::Result<()> {
        if self.panels.contains_key(&slot) {
            return Err(eyre!("duplicate panel contribution for `{slot:?}`"));
        }
        self.panels.insert(slot, render);
        Ok(())
    }

    pub fn register_status(
        &mut self,
        slot: StatusSlot,
        render: RenderContribution,
    ) -> color_eyre::Result<()> {
        if self.status.contains_key(&slot) {
            return Err(eyre!("duplicate status contribution for `{slot:?}`"));
        }
        self.status.insert(slot, render);
        Ok(())
    }

    pub fn register_overlay(
        &mut self,
        id: &'static str,
        provider: OverlayProvider,
    ) -> color_eyre::Result<()> {
        if self.overlays.iter().any(|(candidate, _)| *candidate == id) {
            return Err(eyre!("duplicate overlay contribution `{id}`"));
        }
        self.overlays.push((id, provider));
        Ok(())
    }

    pub fn register_editor_overlay(
        &mut self,
        id: &'static str,
        provider: EditorOverlayProvider,
    ) -> color_eyre::Result<()> {
        if self
            .editor_overlays
            .iter()
            .any(|(candidate, _)| *candidate == id)
        {
            return Err(eyre!("duplicate editor overlay contribution `{id}`"));
        }
        self.editor_overlays.push((id, provider));
        Ok(())
    }

    pub fn render_panel(&self, slot: PanelSlot, model: &Model, frame: &mut Frame, area: Rect) {
        if let Some(render) = self.panels.get(&slot) {
            render(model, frame, area);
        }
    }

    pub fn render_status(&self, slot: StatusSlot, model: &Model, frame: &mut Frame, area: Rect) {
        if let Some(render) = self.status.get(&slot) {
            render(model, frame, area);
        }
    }

    pub fn editor_overlays(&self, model: &Model) -> Vec<EditorOverlayContribution> {
        self.editor_overlays
            .iter()
            .filter_map(|(_, provider)| provider(model))
            .collect()
    }

    pub fn overlays(&self, model: &Model) -> Vec<OverlayContribution> {
        let mut overlays = self
            .overlays
            .iter()
            .filter_map(|(_, provider)| provider(model))
            .collect::<Vec<_>>();
        overlays.sort_by_key(|overlay| overlay.kind);
        overlays
    }
}

fn focus_for(current: FocusTarget, component: FocusTarget) -> FocusState {
    match current == component {
        true => FocusState::Focused,
        false => FocusState::Unfocused,
    }
}

fn render_explorer_panel(model: &Model, frame: &mut Frame, area: Rect) {
    frame.render_widget(
        FileExplorer::new(
            model.workspace.as_ref(),
            &model.explorer,
            focus_for(model.current_focus, FocusTarget::FileExplorer),
            model.theme.ui(),
        ),
        area,
    );
}

fn render_tools_panel(model: &Model, frame: &mut Frame, area: Rect) {
    if model.tools.active_tool == Some(crate::tools::ToolKind::Agent) {
        crate::agent::AgentPanel::render(
            &model.agent,
            model.current_focus,
            model.theme.ui(),
            frame,
            area,
        );
        return;
    }
    if model.tools.active_tool == Some(crate::tools::ToolKind::Diagnostics) {
        frame.render_widget(
            DiagnosticsPanel::new(
                &model.diagnostics,
                model.workspace.as_ref().map(|workspace| workspace.root()),
                focus_for(model.current_focus, FocusTarget::ToolsPanel),
                model.theme.ui(),
            ),
            area,
        );
        return;
    }
    frame.render_widget(
        ToolsPanel::new(
            model.tools.active_tool,
            focus_for(model.current_focus, FocusTarget::ToolsPanel),
            model.theme.ui(),
        ),
        area,
    );
}

fn render_status(model: &Model, frame: &mut Frame, area: Rect) {
    let file_path = active_file_path(model);
    let time = chrono::Local::now().format("%H:%M").to_string();
    let pending_input = model
        .pending_input
        .as_ref()
        .filter(|pending| {
            model
                .context_stack
                .last()
                .is_some_and(|context| pending.is_owner(*context))
        })
        .map(PendingInput::text);
    frame.render_widget(
        StatusBar::new(
            model.editor.mode(),
            model.vcs.head(),
            &file_path,
            &time,
            model.status.as_ref(),
            pending_input,
            model.theme.ui(),
        ),
        area,
    );
}

fn active_file_path(model: &Model) -> String {
    let Some(path) = model
        .editor
        .active_buffer()
        .and_then(|id| model.editor.buffer_path(id))
    else {
        return String::new();
    };
    let path = model
        .workspace
        .as_ref()
        .and_then(|workspace| path.strip_prefix(workspace.root()).ok())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    if crate::capabilities::readonly::active_buffer_is_read_only(model) {
        format!("{path} [read-only]")
    } else {
        path
    }
}

fn command_menu_overlay(model: &Model) -> Option<OverlayContribution> {
    if model.command_menu.items().is_empty() {
        return None;
    }
    let hints = KeyHints::new(&model.command_menu, model.theme.ui());
    Some(OverlayContribution::new(
        "command_menu",
        OverlayKind::Hint,
        OverlayPlacement::BottomRight(hints.size()),
        render_key_hints,
    ))
}

fn render_key_hints(model: &Model, frame: &mut Frame, area: Rect) {
    let hints = KeyHints::new(&model.command_menu, model.theme.ui());
    frame.render_widget(hints, area);
}

fn centered_area(area: Rect, size: Size) -> Rect {
    let width = size.width.min(area.width);
    let height = size.height.min(area.height);
    let [_, popup_band, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .flex(Flex::Start)
    .areas(area);
    let [_, popup, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .flex(Flex::Start)
    .areas(popup_band);
    popup
}

fn bottom_right_area(area: Rect, size: Size) -> Rect {
    let width = size.width.min(area.width);
    let height = size.height.min(area.height.saturating_sub(1));
    let [_, popup_band, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Length(1),
    ])
    .flex(Flex::Start)
    .areas(area);
    let [_, popup] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)])
        .flex(Flex::Start)
        .areas(popup_band);
    popup
}

fn main_panel_area(area: Rect, max_width: u16, max_height: u16, margin: u16) -> Rect {
    let width = area.width.saturating_sub(margin).min(max_width);
    let height = area.height.saturating_sub(margin).min(max_height);
    centered_area(area, Size { width, height })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Workspace,
        test_utils::{create_buffer, open_file},
    };

    #[test]
    fn status_path_is_project_relative_when_possible() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("src");
        std::fs::create_dir(&source).unwrap();
        let path = source.join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = Model::default();
        open_file(&mut model, &path);
        model.workspace = Some(Workspace::new(directory.path().to_path_buf()));
        assert_eq!(active_file_path(&model), "src/main.rs");

        model.workspace = None;
        assert_eq!(active_file_path(&model), path.to_string_lossy());

        let mut untitled = Model::default();
        assert_eq!(active_file_path(&untitled), "");
        create_buffer(&mut untitled, "text");
        assert_eq!(active_file_path(&untitled), "");
    }

    #[test]
    fn external_status_path_is_marked_read_only() {
        let workspace = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let path = external.path().join("dependency.rs");
        std::fs::write(&path, "fn dependency() {}\n").unwrap();
        let mut model = Model {
            workspace: Some(Workspace::new(workspace.path().to_path_buf())),
            ..Model::default()
        };
        open_file(&mut model, &path);

        assert_eq!(
            active_file_path(&model),
            format!("{} [read-only]", path.display())
        );
    }

    #[test]
    fn rejects_duplicate_contribution_slots_and_overlay_ids() {
        fn replacement(_: &Model, _: &mut Frame, _: Rect) {}

        let mut ui = UiRegistry::empty();
        ui.register_panel(PanelSlot::Explorer, render_explorer_panel)
            .unwrap();
        let panel = ui.panels.get(&PanelSlot::Explorer).copied().unwrap() as usize;
        assert!(ui.register_panel(PanelSlot::Explorer, replacement).is_err());
        assert_eq!(
            ui.panels.get(&PanelSlot::Explorer).copied().unwrap() as usize,
            panel
        );
        ui.register_status(StatusSlot::Message, render_status)
            .unwrap();
        let status = ui.status.get(&StatusSlot::Message).copied().unwrap() as usize;
        assert!(
            ui.register_status(StatusSlot::Message, replacement)
                .is_err()
        );
        assert_eq!(
            ui.status.get(&StatusSlot::Message).copied().unwrap() as usize,
            status
        );
        ui.register_overlay("example", command_menu_overlay)
            .unwrap();
        assert!(
            ui.register_overlay("example", command_menu_overlay)
                .is_err()
        );

        fn no_editor_overlay(_: &Model) -> Option<EditorOverlayContribution> {
            None
        }
        ui.register_editor_overlay("example", no_editor_overlay)
            .unwrap();
        assert!(
            ui.register_editor_overlay("example", no_editor_overlay)
                .is_err()
        );
    }

    #[test]
    fn placements_clamp_to_narrow_areas() {
        let centered = centered_area(
            Rect::new(0, 0, 8, 6),
            Size {
                width: 20,
                height: 10,
            },
        );
        assert_eq!(centered.width, 8);
        assert_eq!(centered.height, 6);

        let hints = bottom_right_area(Rect::new(0, 0, 8, 3), Size::new(16, 5));
        assert_eq!(hints.width, 8);
        assert_eq!(hints.height, 2);
        assert_eq!(hints.x + hints.width, 8);
        assert_eq!(hints.y + hints.height, 2);

        let panel = main_panel_area(Rect::new(0, 0, 100, 9), 160, 36, 2);
        assert_eq!(panel.width, 98);
        assert_eq!(panel.height, 7);
        assert_eq!(panel.x, 1);
        assert_eq!(panel.y, 1);

        let large = main_panel_area(Rect::new(0, 0, 200, 50), 160, 36, 2);
        assert_eq!(large.width, 160);
        assert_eq!(large.height, 36);
    }
}
