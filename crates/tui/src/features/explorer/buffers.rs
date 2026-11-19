use std::path::Path;

use editor::BufferId;

use crate::{
    data_object::{Model, StatusMessage},
    filesystem::FileKind,
    manager::effect::Effect,
};

pub(super) fn reject_blocked(
    model: &mut Model,
    path: &Path,
    kind: FileKind,
    action: &str,
) -> Option<Effect> {
    let buffers = affected(model, path, kind);
    buffers
        .iter()
        .any(|&id| model.editor.is_saving(id) || model.editor.is_modified(id) == Some(true))
        .then(|| {
            crate::notifications::report_status(
                model,
                StatusMessage::error(format!(
                    "Save modified buffers and wait for saves before {action}"
                )),
            )
        })
}

pub(super) fn close(model: &mut Model, path: &Path, kind: FileKind) -> Option<Effect> {
    let buffers = affected(model, path, kind);
    model.editor.close_buffers(&buffers).err().map(|error| {
        crate::notifications::report_status(
            model,
            StatusMessage::error(format!(
                "The filesystem operation succeeded, but closing its buffers failed: {error}"
            )),
        )
    })
}

fn affected(model: &Model, path: &Path, kind: FileKind) -> Vec<BufferId> {
    model
        .editor
        .file_buffer_ids()
        .into_iter()
        .filter(|&id| {
            model
                .editor
                .buffer_path(id)
                .is_some_and(|candidate| match kind {
                    FileKind::Directory => candidate.starts_with(path),
                    FileKind::File | FileKind::Link => candidate == path,
                })
        })
        .collect()
}
