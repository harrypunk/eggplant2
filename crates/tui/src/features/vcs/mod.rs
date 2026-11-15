use std::path::PathBuf;

use crate::{
    data_object::{Message, Model},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VcsState {
    head: Option<String>,
    target: Option<PathBuf>,
    revision: u64,
    refreshing: bool,
}

impl VcsState {
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }
}

/// Start one background HEAD lookup, preserving the previous label until completion.
pub fn refresh(model: &mut Model) -> Option<Effect> {
    let target = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
        .or_else(|| {
            model
                .editor
                .active_buffer()
                .and_then(|id| model.editor.buffer_path(id))
                .map(PathBuf::from)
        });
    let Some(target) = target else {
        model.vcs.target = None;
        model.vcs.head = None;
        model.vcs.refreshing = false;
        model.vcs.revision = model.vcs.revision.wrapping_add(1);
        return None;
    };
    if model.vcs.target.as_ref() == Some(&target) && model.vcs.refreshing {
        return None;
    }

    model.vcs.target = Some(target.clone());
    model.vcs.refreshing = true;
    model.vcs.revision = model.vcs.revision.wrapping_add(1);
    Some(Effect::feature(HeadRequest {
        target,
        revision: model.vcs.revision,
    }))
}

struct HeadRequest {
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for HeadRequest {
    fn perform(self: Box<Self>) -> Message {
        let head = ::vcs::current_head(&self.target);
        Message::FeatureCompletion(FeatureCompletion::new(HeadCompletion {
            target: self.target,
            revision: self.revision,
            head,
        }))
    }
}

struct HeadCompletion {
    target: PathBuf,
    revision: u64,
    head: Option<String>,
}

impl Completion for HeadCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.revision != self.revision || model.vcs.target.as_ref() != Some(&self.target) {
            return None;
        }
        model.vcs.refreshing = false;
        model.vcs.head = self.head;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_object::Workspace;

    #[test]
    fn refresh_coalesces_and_completion_rejects_stale_results() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.head = Some("old".to_string());

        assert!(refresh(&mut model).is_some());
        assert!(refresh(&mut model).is_none());
        assert_eq!(model.vcs.head(), Some("old"));
        let revision = model.vcs.revision;

        Box::new(HeadCompletion {
            target: root.clone(),
            revision: revision.wrapping_sub(1),
            head: Some("stale".to_string()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.head(), Some("old"));
        assert!(model.vcs.refreshing);

        Box::new(HeadCompletion {
            target: root,
            revision,
            head: Some("main".to_string()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.head(), Some("main"));
        assert!(!model.vcs.refreshing);

        model.workspace = None;
        assert!(refresh(&mut model).is_none());
        assert_eq!(model.vcs.head(), None);
        assert_eq!(model.vcs.target, None);
    }
}
