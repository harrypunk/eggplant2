use std::{ffi::OsString, path::PathBuf, sync::Arc};

use app::language::{LanguageServerRegistry, LanguageServers};

use color_eyre::eyre::{WrapErr, bail};
use editor::DocumentFactory;

use crate::config::ThemeConfig;

use crate::{
    data_object::{Model, Workspace},
    explorer::{ExplorerEntryKind, ExplorerState},
    file_picker::FileEntry,
    manager::effect::Effect,
};

/// Parse the optional file or folder argument without adding a CLI framework.
pub fn startup_path_from(
    args: impl IntoIterator<Item = OsString>,
) -> color_eyre::Result<Option<PathBuf>> {
    let mut args = args.into_iter();
    let path = args.next().map(PathBuf::from);
    if args.next().is_some() {
        bail!("expected at most one file or folder path; usage: tui [PATH]");
    }
    Ok(path)
}

/// Build the initial application model before terminal raw mode is enabled.
pub fn initial_model(path: Option<PathBuf>) -> color_eyre::Result<Model> {
    initial_model_with_languages(path, Arc::new(LanguageServers::defaults()))
}

/// Compose a model with an externally constructed language-server registry.
pub fn initial_model_with_languages(
    path: Option<PathBuf>,
    language_servers: Arc<dyn LanguageServerRegistry>,
) -> color_eyre::Result<Model> {
    let mut model = Model {
        language_servers,
        ..Model::default()
    };
    model
        .document_lifecycle
        .set_on_close(crate::code_navigation::close_document);

    match path {
        Some(path) if path.is_dir() => {
            let root = std::fs::canonicalize(&path)
                .wrap_err_with(|| format!("failed to open folder `{}`", path.display()))?;
            model.explorer = ExplorerState::load(&root)
                .wrap_err_with(|| format!("failed to read folder `{}`", root.display()))?;
            let picker_files = model
                .explorer
                .entries
                .iter()
                .filter(|entry| entry.kind != ExplorerEntryKind::Directory && entry.path.is_file())
                .map(|entry| FileEntry {
                    path: entry.path.clone(),
                    display: entry.name.to_string_lossy().into_owned(),
                })
                .collect();
            model.file_picker.seed_top_level(picker_files);
            model.workspace = Some(Workspace::new(root));
        }
        Some(path) => {
            let document = DocumentFactory::new()
                .open(&path)
                .wrap_err_with(|| format!("failed to open `{}`", path.display()))?;
            model.editor.insert_document(document);
        }
        None => {}
    }

    Ok(model)
}

/// Compose initial feature work without embedding optional addons in the terminal runtime.
pub fn initial_effects(model: &mut Model, source: &ThemeConfig) -> Option<Effect> {
    Effect::combine(
        crate::code_navigation::synchronize(model),
        Effect::combine(
            crate::theme::start_loading(source, model),
            Effect::combine(
                crate::vcs::refresh(model),
                crate::vcs::sync_active_buffer(model),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use app::language::LanguageServerSpec;

    use super::*;
    use crate::{
        data_object::{Context, FocusTarget},
        file_picker::FileCollectionState,
        manager::input_context::input_context,
    };

    #[test]
    fn no_path_starts_with_empty_editor() {
        let model = initial_model(None).unwrap();

        assert!(model.editor.is_empty());
        assert!(model.workspace.is_none());
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(input_context(&model), Some(Context::Welcome));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn file_path_opens_file_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();

        let model = initial_model(Some(path)).unwrap();

        assert_eq!(model.editor.buffer_count(), 1);
        assert!(model.editor.active_buffer().is_some());
        assert!(model.workspace.is_none());
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(input_context(&model), Some(Context::AppNormal));
    }

    #[test]
    fn language_registry_is_injected_at_composition() {
        let mut servers = LanguageServers::default();
        servers
            .register(
                LanguageServerSpec::new("typescript", "typescript-language-server", ["--stdio"])
                    .with_extensions(["ts", "tsx"]),
            )
            .unwrap();

        let model = initial_model_with_languages(None, Arc::new(servers)).unwrap();
        let spec = model
            .language_servers
            .find_for_path(std::path::Path::new("main.ts"))
            .unwrap();
        assert_eq!(spec.command(), "typescript-language-server");
        assert_eq!(spec.args(), &[String::from("--stdio")]);
    }

    #[test]
    fn folder_path_opens_workspace_and_focuses_welcome_without_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let expected_root = std::fs::canonicalize(directory.path()).unwrap();
        std::fs::write(directory.path().join("note.txt"), "hello").unwrap();

        let model = initial_model(Some(directory.path().to_path_buf())).unwrap();

        assert!(model.editor.is_empty());
        assert_eq!(model.editor.active_buffer(), None);
        assert_eq!(model.workspace.as_ref().unwrap().root(), expected_root);
        assert_eq!(model.explorer.entries.len(), 1);
        assert_eq!(model.explorer.selected, Some(0));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(input_context(&model), Some(Context::Welcome));
        assert!(model.context_stack.is_empty());
        assert_eq!(model.file_picker.collection, FileCollectionState::Ready);
    }

    #[test]
    fn missing_file_path_creates_new_file_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new.txt");

        let model = initial_model(Some(path.clone())).unwrap();

        assert!(model.editor.active_buffer().is_some());
        let display_name = model.editor.active_snapshot().unwrap().display_name;
        assert_eq!(
            std::path::Path::new(&display_name).file_name().unwrap(),
            "new.txt"
        );
        assert!(model.workspace.is_none());
    }

    #[test]
    fn rejects_multiple_paths() {
        let args = [OsString::from("first"), OsString::from("second")];
        let error = startup_path_from(args).unwrap_err();

        assert!(error.to_string().contains("at most one"));
    }
}
