use std::path::PathBuf;

use color_eyre::eyre::WrapErr;
use editor::DocumentFactory;
use helix_view::Document;

use crate::{
    data_object::{Model, Workspace},
    explorer::{ExplorerEntryKind, ExplorerState},
    file_picker::FileEntry,
};

/// Semantic startup content produced by a loader, independent of model composition.
pub enum StartupSource {
    Empty,
    Workspace(Box<WorkspaceSnapshot>),
    Document(Box<Document>),
}

impl StartupSource {
    pub fn install(self, mut model: Model) -> Model {
        match self {
            Self::Empty => {}
            Self::Workspace(snapshot) => snapshot.install(&mut model),
            Self::Document(document) => {
                model.editor.insert_document(*document);
            }
        }
        model
    }
}

pub struct WorkspaceSnapshot {
    workspace: Workspace,
    explorer: ExplorerState,
    picker_files: Vec<FileEntry>,
}

impl WorkspaceSnapshot {
    pub fn new(
        workspace: Workspace,
        explorer: ExplorerState,
        picker_files: Vec<FileEntry>,
    ) -> Self {
        Self {
            workspace,
            explorer,
            picker_files,
        }
    }

    fn install(self, model: &mut Model) {
        model.explorer = self.explorer;
        model.file_picker.seed_top_level(self.picker_files);
        model.workspace = Some(self.workspace);
    }
}

/// Loading boundary that keeps filesystem and parser details out of model assembly.
pub trait StartupSourceLoader {
    fn load(&self, path: Option<PathBuf>) -> color_eyre::Result<StartupSource>;
}

#[derive(Debug, Default)]
pub struct FilesystemStartupLoader;

impl StartupSourceLoader for FilesystemStartupLoader {
    fn load(&self, path: Option<PathBuf>) -> color_eyre::Result<StartupSource> {
        match path {
            Some(path) if path.is_dir() => {
                load_workspace(path).map(|snapshot| StartupSource::Workspace(Box::new(snapshot)))
            }
            Some(path) => {
                load_document(path).map(|document| StartupSource::Document(Box::new(document)))
            }
            None => Ok(StartupSource::Empty),
        }
    }
}

fn load_workspace(path: PathBuf) -> color_eyre::Result<WorkspaceSnapshot> {
    let root = std::fs::canonicalize(&path)
        .wrap_err_with(|| format!("failed to open folder `{}`", path.display()))?;
    let explorer = ExplorerState::load(&root)
        .wrap_err_with(|| format!("failed to read folder `{}`", root.display()))?;
    let picker_files = explorer
        .entries
        .iter()
        .filter(|entry| entry.kind != ExplorerEntryKind::Directory && entry.path.is_file())
        .map(|entry| FileEntry {
            path: entry.path.clone(),
            display: entry.name.to_string_lossy().into_owned(),
        })
        .collect();
    Ok(WorkspaceSnapshot::new(
        Workspace::new(root),
        explorer,
        picker_files,
    ))
}

fn load_document(path: PathBuf) -> color_eyre::Result<Document> {
    DocumentFactory::new()
        .open(&path)
        .wrap_err_with(|| format!("failed to open `{}`", path.display()))
}
