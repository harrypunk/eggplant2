use std::path::Path;

use crate::data_object::{Model, Workspace};

/// File-backed buffers outside the fixed workspace are navigation-only.
pub fn active_buffer_is_read_only(model: &Model) -> bool {
    let Some(workspace) = model.workspace.as_ref() else {
        return false;
    };
    model
        .editor
        .active_buffer()
        .and_then(|id| model.editor.buffer_path(id))
        .is_some_and(|path| is_external_path(workspace, path))
}

fn is_external_path(workspace: &Workspace, path: &Path) -> bool {
    !path.starts_with(workspace.root())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn only_paths_outside_the_workspace_root_are_read_only() {
        let workspace = Workspace::new(PathBuf::from("/workspace"));

        assert!(!is_external_path(
            &workspace,
            Path::new("/workspace/src/main.rs")
        ));
        assert!(is_external_path(&workspace, Path::new("/registry/lib.rs")));
        assert!(is_external_path(
            &workspace,
            Path::new("/workspace-other/main.rs")
        ));
    }
}
