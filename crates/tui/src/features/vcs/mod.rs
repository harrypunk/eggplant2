use std::path::PathBuf;

use ::vcs::CommitSummary;

use crate::{
    data_object::{AppPage, CommandId, Context, Message, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
};

const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VcsState {
    head: Option<String>,
    target: Option<PathBuf>,
    revision: u64,
    refreshing: bool,
    pub(crate) history: HistoryState,
    history_target: Option<PathBuf>,
    history_revision: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum HistoryState {
    #[default]
    Idle,
    Loading,
    Ready(Vec<CommitSummary>),
    Failed(String),
}

impl VcsState {
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    pub fn history(&self) -> &HistoryState {
        &self.history
    }
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("open_vcs_page"),
            "open Git history",
            handle_command,
        ),
        Command::feature(
            CommandId("close_vcs_page"),
            "close Git page",
            handle_command,
        ),
        Command::feature(
            CommandId("close_vcs_page_q"),
            "close Git page",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "open_vcs_page" => open_page(model),
        "close_vcs_page" | "close_vcs_page_q" => close_page(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown VCS command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

fn open_page(model: &mut Model) -> Option<Effect> {
    if model.active_page != AppPage::Editor {
        return None;
    }
    model.active_page = AppPage::Vcs;
    model.context_stack.push(Context::VCS);
    Effect::combine(refresh(model), refresh_history(model))
}

fn close_page(model: &mut Model) -> Option<Effect> {
    model.active_page = AppPage::Editor;
    model
        .context_stack
        .pop_if(|context| context == &Context::VCS);
    None
}

/// Start one background HEAD lookup, preserving the previous label until completion.
pub fn refresh(model: &mut Model) -> Option<Effect> {
    let target = repository_target(model);
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

fn repository_target(model: &Model) -> Option<PathBuf> {
    model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
        .or_else(|| {
            model
                .editor
                .active_buffer()
                .and_then(|id| model.editor.buffer_path(id))
                .map(PathBuf::from)
        })
}

fn refresh_history(model: &mut Model) -> Option<Effect> {
    let Some(target) = repository_target(model) else {
        model.vcs.history_target = None;
        model.vcs.history = HistoryState::Failed("not a Git repository".to_string());
        model.vcs.history_revision = model.vcs.history_revision.wrapping_add(1);
        return None;
    };
    if model.vcs.history_target.as_ref() == Some(&target)
        && model.vcs.history == HistoryState::Loading
    {
        return None;
    }

    model.vcs.history_target = Some(target.clone());
    model.vcs.history = HistoryState::Loading;
    model.vcs.history_revision = model.vcs.history_revision.wrapping_add(1);
    Some(Effect::feature(HistoryRequest {
        target,
        revision: model.vcs.history_revision,
    }))
}

struct HistoryRequest {
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for HistoryRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::history(&self.target, HISTORY_LIMIT).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(HistoryCompletion {
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct HistoryCompletion {
    target: PathBuf,
    revision: u64,
    result: Result<Vec<CommitSummary>, String>,
}

impl Completion for HistoryCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.history_revision != self.revision
            || model.vcs.history_target.as_ref() != Some(&self.target)
        {
            return None;
        }
        model.vcs.history = match self.result {
            Ok(commits) => HistoryState::Ready(commits),
            Err(error) => HistoryState::Failed(error),
        };
        None
    }
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

    #[test]
    fn page_open_and_close_restores_the_editor_context() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

        assert!(handle_command(&mut model, CommandId("open_vcs_page")).is_none());
        assert_eq!(model.active_page, AppPage::Vcs);
        assert_eq!(model.context_stack.last(), Some(&Context::VCS));
        assert_eq!(
            model.vcs.history(),
            &HistoryState::Failed("not a Git repository".to_string())
        );

        assert!(handle_command(&mut model, CommandId("close_vcs_page")).is_none());
        assert_eq!(model.active_page, AppPage::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn history_refresh_coalesces_and_rejects_stale_results() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        assert!(refresh_history(&mut model).is_some());
        assert!(refresh_history(&mut model).is_none());
        let revision = model.vcs.history_revision;

        Box::new(HistoryCompletion {
            target: root.clone(),
            revision: revision.wrapping_sub(1),
            result: Ok(Vec::new()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.history(), &HistoryState::Loading);

        let commits = vec![CommitSummary {
            id: "abc1234".to_string(),
            subject: "subject".to_string(),
        }];
        Box::new(HistoryCompletion {
            target: root,
            revision,
            result: Ok(commits.clone()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.history(), &HistoryState::Ready(commits));
    }
}
