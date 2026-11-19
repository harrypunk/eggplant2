use std::path::Path;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::UnboundedSender;

use crate::{data_object::Message, external_sync::ExternalChangeKind};

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
            if !is_file_change(&event.kind) {
                return;
            }
            let kind = change_kind(&event.kind);
            event.paths.into_iter().for_each(|path| {
                let path = std::fs::canonicalize(&path).unwrap_or(path);
                let _ = sender.send(Message::ExternalFileChanged { path, kind });
            });
        },
        notify::Config::default(),
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(Some(FileWatcher { _watcher: watcher }))
}

fn is_file_change(kind: &EventKind) -> bool {
    let (EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) =
        kind
    else {
        return false;
    };
    true
}

fn change_kind(kind: &EventKind) -> ExternalChangeKind {
    match kind {
        EventKind::Any
        | EventKind::Create(_)
        | EventKind::Remove(_)
        | EventKind::Modify(notify::event::ModifyKind::Name(_)) => ExternalChangeKind::Structure,
        _ => ExternalChangeKind::Content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_and_structure_events_share_one_watcher() {
        assert!(is_file_change(&EventKind::Any));
        assert!(is_file_change(&EventKind::Create(
            notify::event::CreateKind::Any
        )));
        assert!(is_file_change(&EventKind::Modify(
            notify::event::ModifyKind::Any
        )));
        assert!(is_file_change(&EventKind::Remove(
            notify::event::RemoveKind::Any
        )));
        assert!(!is_file_change(&EventKind::Access(
            notify::event::AccessKind::Any
        )));

        assert_eq!(
            change_kind(&EventKind::Create(notify::event::CreateKind::Any)),
            ExternalChangeKind::Structure
        );
        assert_eq!(
            change_kind(&EventKind::Remove(notify::event::RemoveKind::Any)),
            ExternalChangeKind::Structure
        );
        assert_eq!(
            change_kind(&EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Any
            ))),
            ExternalChangeKind::Structure
        );
        assert_eq!(
            change_kind(&EventKind::Modify(notify::event::ModifyKind::Any)),
            ExternalChangeKind::Content
        );
    }
}
