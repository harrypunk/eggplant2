use std::path::Path;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::UnboundedSender;

use crate::data_object::Message;

/// Keep the OS watcher alive for the application's runtime scope.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

pub fn start(
    root: Option<&Path>,
    sender: UnboundedSender<Message>,
) -> color_eyre::Result<Option<FileWatcher>> {
    let Some(root) = root else {
        return Ok(None);
    };
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| {
            let Ok(event) = result else {
                return;
            };
            if !is_file_update(&event.kind) {
                return;
            }
            event.paths.into_iter().for_each(|path| {
                let path = std::fs::canonicalize(&path).unwrap_or(path);
                let _ = sender.send(Message::ExternalFileChanged(path));
            });
        },
        notify::Config::default(),
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(Some(FileWatcher { _watcher: watcher }))
}

fn is_file_update(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_updates_and_creation_trigger_reload_checks() {
        assert!(is_file_update(&EventKind::Any));
        assert!(is_file_update(&EventKind::Create(
            notify::event::CreateKind::Any
        )));
        assert!(is_file_update(&EventKind::Modify(
            notify::event::ModifyKind::Any
        )));
        assert!(!is_file_update(&EventKind::Remove(
            notify::event::RemoveKind::Any
        )));
        assert!(!is_file_update(&EventKind::Access(
            notify::event::AccessKind::Any
        )));
    }
}
