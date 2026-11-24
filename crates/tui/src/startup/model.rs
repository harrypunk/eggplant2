use std::{path::PathBuf, sync::Arc};

use app::language::{LanguageServerRegistry, LanguageServers};

use super::source::{FilesystemStartupLoader, StartupSourceLoader};
use crate::data_object::Model;

/// Build the initial application model before terminal raw mode is enabled.
pub fn initial_model(path: Option<PathBuf>) -> color_eyre::Result<Model> {
    initial_model_with_languages(path, Arc::new(LanguageServers::defaults()))
}

/// Compose a model with externally constructed language and startup-source interfaces.
pub fn initial_model_with_languages(
    path: Option<PathBuf>,
    language_servers: Arc<dyn LanguageServerRegistry>,
) -> color_eyre::Result<Model> {
    initial_model_with_loader(path, language_servers, &FilesystemStartupLoader)
}

/// Assemble a model from semantic startup content and injected dependencies.
pub fn initial_model_with_loader(
    path: Option<PathBuf>,
    language_servers: Arc<dyn LanguageServerRegistry>,
    loader: &dyn StartupSourceLoader,
) -> color_eyre::Result<Model> {
    let source = loader.load(path)?;
    let mut model = Model {
        language_servers,
        ..Model::default()
    };
    model
        .document_lifecycle
        .set_on_close(crate::code_navigation::close_document);
    Ok(source.install(model))
}

#[cfg(test)]
mod tests {
    use app::language::LanguageServerSpec;

    use super::*;
    use crate::{
        data_object::{Context, FocusTarget},
        file_picker::FileCollectionState,
        manager::input_context::input_context,
        startup::StartupSource,
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
    fn startup_source_loader_is_replaceable() {
        struct EmptyLoader;

        impl StartupSourceLoader for EmptyLoader {
            fn load(&self, _path: Option<PathBuf>) -> color_eyre::Result<StartupSource> {
                Ok(StartupSource::Empty)
            }
        }

        let model = initial_model_with_loader(
            Some(PathBuf::from("/path/that/does/not/exist")),
            Arc::new(LanguageServers::default()),
            &EmptyLoader,
        )
        .unwrap();

        assert!(model.editor.is_empty());
        assert!(model.workspace.is_none());
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
}
