use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use app::language::{Diagnostic, DiagnosticBatch};

use crate::{
    data_object::{CommandId, Model, StatusMessage, Visibility},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect},
    },
    tools::ToolKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsUpdate {
    Applied,
    Ignored,
}

/// Provider-neutral diagnostic batches indexed by canonical document path.
#[derive(Debug, Default)]
pub struct DiagnosticsState {
    batches: BTreeMap<PathBuf, DiagnosticBatch>,
    server_generation: u64,
}

impl DiagnosticsState {
    pub fn replace(
        &mut self,
        batch: DiagnosticBatch,
        server_generation: u64,
        current_version: Option<i32>,
    ) -> DiagnosticsUpdate {
        if server_generation != self.server_generation {
            return DiagnosticsUpdate::Ignored;
        }
        if batch
            .version
            .zip(current_version)
            .is_some_and(|(incoming, current)| incoming < current)
        {
            return DiagnosticsUpdate::Ignored;
        }

        if batch.diagnostics.is_empty() {
            self.batches.remove(&batch.path);
        } else {
            self.batches.insert(batch.path.clone(), batch);
        }
        DiagnosticsUpdate::Applied
    }

    pub fn restart_server(&mut self) {
        self.server_generation = self.server_generation.wrapping_add(1);
        self.batches.clear();
    }

    pub fn server_generation(&self) -> u64 {
        self.server_generation
    }

    pub fn clear_path(&mut self, path: &Path) {
        self.batches.remove(path);
    }

    pub fn all(&self) -> impl Iterator<Item = &Diagnostic> {
        self.batches
            .values()
            .flat_map(|batch| batch.diagnostics.iter())
    }
}

/// Show the non-modal diagnostics panel without moving editor focus.
pub fn toggle_panel(model: &mut Model) {
    if model.tools.visibility == Visibility::Displayed
        && model.tools.active_tool == Some(ToolKind::Diagnostics)
    {
        model.tools.visibility = Visibility::Hidden;
    } else {
        model.tools.active_tool = Some(ToolKind::Diagnostics);
        model.tools.visibility = Visibility::Displayed;
    }
}

pub struct DiagnosticsCompletion {
    pub batch: DiagnosticBatch,
    pub server_generation: u64,
}

impl Completion for DiagnosticsCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let current_version = model
            .editor
            .file_buffer_ids()
            .into_iter()
            .find(|&id| model.editor.buffer_path(id) == Some(self.batch.path.as_path()))
            .and_then(|id| model.editor.buffer_version(id));
        model
            .diagnostics
            .replace(self.batch, self.server_generation, current_version);
        None
    }
}

pub struct DiagnosticFailureCompletion {
    pub message: String,
    pub server_generation: u64,
    pub fatal: bool,
}

impl Completion for DiagnosticFailureCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if self.server_generation != model.diagnostics.server_generation() {
            return None;
        }
        if self.fatal {
            model.code_navigation.invalidate_service();
            model.diagnostics.restart_server();
        }
        Some(crate::notifications::report_status_titled(
            model,
            StatusMessage::error(self.message),
            Some("LSP"),
        ))
    }
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    commands.register(Command::feature(
        CommandId("code_diagnostics"),
        "show diagnostics",
        handle_command,
    ))
}

fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    if id == CommandId("code_diagnostics") {
        toggle_panel(model);
        return None;
    }
    Some(crate::notifications::report_status(
        model,
        StatusMessage::error(format!("unknown diagnostics command `{}`", id.as_str())),
    ))
}

#[cfg(test)]
mod tests {
    use app::language::DiagnosticSeverity;
    use editor::DocumentPosition;

    use super::*;

    fn batch(path: &str, version: Option<i32>, message: &str) -> DiagnosticBatch {
        DiagnosticBatch {
            path: PathBuf::from(path),
            version,
            diagnostics: vec![Diagnostic {
                path: PathBuf::from(path),
                start: DocumentPosition {
                    line: 0,
                    character: 0,
                },
                end: DocumentPosition {
                    line: 0,
                    character: 1,
                },
                severity: DiagnosticSeverity::Error,
                message: message.to_string(),
                source: None,
            }],
        }
    }

    #[test]
    fn replaces_batches_and_empty_batches_clear_paths() {
        let mut state = DiagnosticsState::default();
        assert_eq!(
            state.replace(batch("/workspace/a.rs", Some(2), "first"), 0, Some(2)),
            DiagnosticsUpdate::Applied
        );
        assert_eq!(
            state.replace(batch("/workspace/b.rs", None, "second"), 0, None),
            DiagnosticsUpdate::Applied
        );
        assert_eq!(state.all().count(), 2);

        let cleared = DiagnosticBatch {
            path: PathBuf::from("/workspace/a.rs"),
            version: None,
            diagnostics: Vec::new(),
        };
        assert_eq!(
            state.replace(cleared, 0, Some(3)),
            DiagnosticsUpdate::Applied
        );
        assert_eq!(state.all().count(), 1);
        assert_eq!(state.all().next().unwrap().message, "second");
        state.clear_path(std::path::Path::new("/workspace/b.rs"));
        assert_eq!(state.all().count(), 0);
    }

    #[test]
    fn stale_document_and_server_generations_are_ignored() {
        let mut state = DiagnosticsState::default();
        assert_eq!(
            state.replace(batch("/workspace/a.rs", Some(2), "old"), 0, Some(3)),
            DiagnosticsUpdate::Ignored
        );
        assert_eq!(
            state.replace(batch("/workspace/a.rs", None, "old server"), 1, None),
            DiagnosticsUpdate::Ignored
        );
        assert_eq!(state.all().count(), 0);

        state.restart_server();
        assert_eq!(
            state.replace(batch("/workspace/a.rs", None, "current"), 1, None),
            DiagnosticsUpdate::Applied
        );
        assert_eq!(state.all().count(), 1);
    }

    #[test]
    fn completion_checks_the_current_open_buffer_version() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}").unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let id = crate::test_utils::open_file(&mut model, &path);
        let version = model.editor.buffer_version(id).unwrap();

        let stale = Box::new(DiagnosticsCompletion {
            batch: batch(path.to_str().unwrap(), Some(version - 1), "stale"),
            server_generation: 0,
        });
        assert!(stale.update(&mut model).is_none());
        assert_eq!(model.diagnostics.all().count(), 0);

        Box::new(DiagnosticsCompletion {
            batch: batch(path.to_str().unwrap(), Some(version), "current"),
            server_generation: 0,
        })
        .update(&mut model);
        assert_eq!(model.diagnostics.all().count(), 1);
    }

    #[test]
    fn panel_toggle_preserves_nonmodal_focus() {
        let mut model = Model::default();
        toggle_panel(&mut model);
        assert_eq!(model.tools.visibility, Visibility::Displayed);
        assert_eq!(model.tools.active_tool, Some(ToolKind::Diagnostics));
        assert_eq!(model.current_focus, crate::data_object::FocusTarget::Editor);

        toggle_panel(&mut model);
        assert_eq!(model.tools.visibility, Visibility::Hidden);
        assert_eq!(model.tools.active_tool, Some(ToolKind::Diagnostics));
    }
}
