use std::path::{Path, PathBuf};

use ::vcs::ResetTarget;

use crate::{
    component::text_input::{TextInput, TextInputChange, TextInputEvent},
    data_object::{CommandId, Context, Message, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        text_input_registry::TextInputRegistry,
    },
};

use super::{report, repository_changed, repository_target};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ResetPhase {
    #[default]
    Closed,
    Menu,
    Loading,
    Selecting,
    Resetting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResetCompletionOutcome {
    Ignored,
    Succeeded(String),
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct ResetState {
    phase: ResetPhase,
    query: TextInput,
    targets: Vec<ResetTarget>,
    selected: Option<usize>,
    target: Option<String>,
    error: Option<String>,
    repository: Option<PathBuf>,
    targets_revision: u64,
    revision: u64,
}

impl ResetState {
    pub(crate) fn is_menu(&self) -> bool {
        self.phase == ResetPhase::Menu
    }

    pub(crate) fn is_popup_active(&self) -> bool {
        [
            ResetPhase::Loading,
            ResetPhase::Selecting,
            ResetPhase::Resetting,
        ]
        .contains(&self.phase)
    }

    pub(crate) fn is_loading(&self) -> bool {
        self.phase == ResetPhase::Loading
    }

    pub(crate) fn is_resetting(&self) -> bool {
        self.phase == ResetPhase::Resetting
    }

    pub(crate) fn is_selecting(&self) -> bool {
        self.phase == ResetPhase::Selecting
    }

    pub(crate) fn query_state(&self) -> &TextInput {
        &self.query
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub(crate) fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn revision_query(&self) -> Option<&str> {
        if !self.is_selecting() {
            return None;
        }
        let query = self.query.trim();
        (query.len() >= 4 && query.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(query)
    }

    pub(crate) fn filtered_targets(&self) -> Vec<&str> {
        let query = self.query.to_lowercase();
        self.targets
            .iter()
            .filter(|target| target.label.to_lowercase().contains(&query))
            .map(|target| target.label.as_str())
            .collect()
    }

    fn open_menu(&mut self) -> bool {
        if self.phase != ResetPhase::Closed {
            return false;
        }
        self.phase = ResetPhase::Menu;
        self.error = None;
        true
    }

    fn start(&mut self, repository: PathBuf) -> Option<u64> {
        if self.phase != ResetPhase::Menu {
            return None;
        }
        self.phase = ResetPhase::Loading;
        self.query.clear();
        self.targets.clear();
        self.selected = None;
        self.target = None;
        self.error = None;
        self.repository = Some(repository);
        self.targets_revision = self.targets_revision.wrapping_add(1);
        Some(self.targets_revision)
    }

    fn cancel(&mut self) -> bool {
        if self.is_resetting() {
            return false;
        }
        let active = self.phase != ResetPhase::Closed;
        self.phase = ResetPhase::Closed;
        self.query.clear();
        self.targets.clear();
        self.selected = None;
        self.target = None;
        self.error = None;
        active
    }

    fn handle_event(&mut self, event: TextInputEvent) -> Option<TextInputChange> {
        self.is_selecting().then(|| {
            let change = self.query.handle_event(event);
            if change.content_changed() {
                self.error = None;
                self.select_first();
            }
            change
        })
    }

    fn select(&mut self, next: bool) {
        if !self.is_selecting() {
            return;
        }
        let len = self.filtered_targets().len();
        if len == 0 {
            self.selected = None;
            return;
        }
        let selected = self.selected.unwrap_or(0);
        self.selected = Some(if next {
            (selected + 1).min(len - 1)
        } else {
            selected.saturating_sub(1)
        });
    }

    fn submit(&mut self, repository: PathBuf) -> Option<(String, u64)> {
        if !self.is_selecting() {
            return None;
        }
        let target = self
            .revision_query()
            .map(str::to_string)
            .or_else(|| self.selected_revision());
        let Some(target) = target else {
            self.error = Some("Select a ref or enter a commit hash".to_string());
            return None;
        };
        self.phase = ResetPhase::Resetting;
        self.target = Some(target.clone());
        self.error = None;
        self.repository = Some(repository);
        self.revision = self.revision.wrapping_add(1);
        Some((target, self.revision))
    }

    fn selected_revision(&self) -> Option<String> {
        let query = self.query.to_lowercase();
        self.targets
            .iter()
            .filter(|target| target.label.to_lowercase().contains(&query))
            .nth(self.selected?)
            .map(|target| target.revision.clone())
    }

    fn select_first(&mut self) {
        self.selected = (!self.filtered_targets().is_empty()).then_some(0);
    }

    fn complete_targets(
        &mut self,
        repository: PathBuf,
        revision: u64,
        result: Result<Vec<ResetTarget>, String>,
    ) {
        if !self.is_loading()
            || self.targets_revision != revision
            || self.repository.as_ref() != Some(&repository)
        {
            return;
        }
        self.phase = ResetPhase::Selecting;
        match result {
            Ok(targets) => {
                self.targets = targets;
                self.select_first();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn complete_reset(
        &mut self,
        repository: &Path,
        revision: u64,
        result: Result<String, String>,
    ) -> ResetCompletionOutcome {
        if !self.is_resetting()
            || self.revision != revision
            || self.repository.as_deref() != Some(repository)
        {
            return ResetCompletionOutcome::Ignored;
        }
        match result {
            Ok(target) => {
                self.phase = ResetPhase::Closed;
                self.query.clear();
                self.targets.clear();
                self.selected = None;
                self.target = None;
                self.error = None;
                ResetCompletionOutcome::Succeeded(target)
            }
            Err(error) => {
                self.phase = ResetPhase::Selecting;
                self.target = None;
                self.error = Some(error);
                ResetCompletionOutcome::Failed
            }
        }
    }
}

pub(super) fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(CommandId("vcs_reset_menu"), "reset commits", handle_command),
        Command::feature(
            CommandId("vcs_reset_menu_close"),
            "close reset menu",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_reset_soft"),
            "soft reset to ref or commit",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_reset_cancel"),
            "cancel soft reset",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_reset_submit"),
            "submit soft reset",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_reset_next"),
            "select next reset target",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_reset_previous"),
            "select previous reset target",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub(crate) fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register_with_cursor(Context::VcsReset, handle_input)
}

pub(super) fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "vcs_reset_menu" => open_menu(model),
        "vcs_reset_menu_close" => close_menu(model),
        "vcs_reset_soft" => start(model),
        "vcs_reset_cancel" => cancel(model),
        "vcs_reset_submit" => submit(model),
        "vcs_reset_next" => select(model, true),
        "vcs_reset_previous" => select(model, false),
        _ => None,
    }
}

fn open_menu(model: &mut Model) -> Option<Effect> {
    if !model.vcs.reset.open_menu() {
        return None;
    }
    model.vcs.help_visible = false;
    model.context_stack.push(Context::VcsResetMenu);
    None
}

fn close_menu(model: &mut Model) -> Option<Effect> {
    if !model.vcs.reset.cancel() {
        return None;
    }
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsResetMenu);
    None
}

fn start(model: &mut Model) -> Option<Effect> {
    if model.vcs.changes.operation_in_progress() {
        return Some(report(
            model,
            StatusMessage::info("Git operation in progress"),
        ));
    }
    let repository = repository_target(model)?;
    if !model.modal.open(Context::VcsReset) {
        return None;
    }
    let Some(revision) = model.vcs.reset.start(repository.clone()) else {
        model.modal.close(Context::VcsReset);
        return None;
    };
    model.vcs.help_visible = false;
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsResetMenu);
    model.context_stack.push(Context::VcsReset);
    Some(Effect::feature(ResetTargetsRequest {
        repository,
        revision,
    }))
}

fn cancel(model: &mut Model) -> Option<Effect> {
    if !model.vcs.reset.cancel() {
        return None;
    }
    close_reset_context(model);
    None
}

fn close_reset_context(model: &mut Model) {
    model.modal.close(Context::VcsReset);
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsReset);
}

fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    model.vcs.reset.handle_event(event);
    None
}

fn select(model: &mut Model, next: bool) -> Option<Effect> {
    model.vcs.reset.select(next);
    None
}

fn submit(model: &mut Model) -> Option<Effect> {
    let repository = repository_target(model)?;
    let (target, revision) = model.vcs.reset.submit(repository.clone())?;
    Some(Effect::feature(ResetRequest {
        repository,
        target,
        revision,
    }))
}

struct ResetTargetsRequest {
    repository: PathBuf,
    revision: u64,
}

impl FeatureEffect for ResetTargetsRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::reset_targets(&self.repository).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(ResetTargetsCompletion {
            repository: self.repository,
            revision: self.revision,
            result,
        }))
    }
}

pub(super) struct ResetTargetsCompletion {
    pub(super) repository: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<Vec<ResetTarget>, String>,
}

impl Completion for ResetTargetsCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model
            .vcs
            .reset
            .complete_targets(self.repository, self.revision, self.result);
        None
    }
}

struct ResetRequest {
    repository: PathBuf,
    target: String,
    revision: u64,
}

impl FeatureEffect for ResetRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::reset_soft(&self.repository, &self.target)
            .map(|id| id.to_hex_with_len(7).to_string())
            .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(ResetCompletion {
            repository: self.repository,
            revision: self.revision,
            result,
        }))
    }
}

pub(super) struct ResetCompletion {
    pub(super) repository: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<String, String>,
}

impl Completion for ResetCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match model
            .vcs
            .reset
            .complete_reset(&self.repository, self.revision, self.result)
        {
            ResetCompletionOutcome::Ignored => None,
            ResetCompletionOutcome::Failed => None,
            ResetCompletionOutcome::Succeeded(target) => {
                close_reset_context(model);
                Effect::combine(
                    repository_changed(model),
                    Some(report(
                        model,
                        StatusMessage::info(format!("Soft reset to {target}")),
                    )),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_object::Workspace;

    fn model() -> Model {
        Model {
            active_page: crate::data_object::AppPage::Vcs,
            workspace: Some(Workspace::new(PathBuf::from("/workspace"))),
            ..Model::default()
        }
    }

    fn start(model: &mut Model) -> u64 {
        handle_command(model, CommandId("vcs_reset_menu"));
        assert!(model.vcs.reset.is_menu());
        assert!(handle_command(model, CommandId("vcs_reset_soft")).is_some());
        assert!(model.vcs.reset.is_loading());
        assert_eq!(model.context_stack.last(), Some(&Context::VcsReset));
        model.vcs.reset.targets_revision
    }

    fn targets(model: &mut Model, revision: u64) {
        Box::new(ResetTargetsCompletion {
            repository: PathBuf::from("/workspace"),
            revision,
            result: Ok(vec![
                ResetTarget {
                    label: "main".to_string(),
                    revision: "main".to_string(),
                },
                ResetTarget {
                    label: "origin/main".to_string(),
                    revision: "origin/main".to_string(),
                },
                ResetTarget {
                    label: "abc1234 recent commit".to_string(),
                    revision: "abc1234".to_string(),
                },
            ]),
        })
        .update(model);
        assert!(model.vcs.reset.is_selecting());
    }

    #[test]
    fn typed_hash_recovers_from_failure_and_completes_soft_reset() {
        let mut model = model();
        let revision = start(&mut model);
        targets(&mut model, revision);
        for character in "abcd".chars() {
            handle_input(&mut model, TextInputEvent::Insert(character));
        }
        assert_eq!(model.vcs.reset.revision_query(), Some("abcd"));
        assert!(handle_command(&mut model, CommandId("vcs_reset_submit")).is_some());
        assert!(model.vcs.reset.is_resetting());
        assert_eq!(model.vcs.reset.target(), Some("abcd"));
        assert!(handle_command(&mut model, CommandId("vcs_reset_cancel")).is_none());

        let revision = model.vcs.reset.revision;
        Box::new(ResetCompletion {
            repository: PathBuf::from("/workspace"),
            revision,
            result: Err("unknown revision".to_string()),
        })
        .update(&mut model);
        assert!(model.vcs.reset.is_selecting());
        assert_eq!(model.vcs.reset.error(), Some("unknown revision"));

        handle_command(&mut model, CommandId("vcs_reset_submit"));
        let completion = Box::new(ResetCompletion {
            repository: PathBuf::from("/workspace"),
            revision: model.vcs.reset.revision,
            result: Ok("abcd".to_string()),
        });
        assert!(matches!(
            completion.update(&mut model),
            Some(Effect::Batch(_))
        ));
        assert!(!model.vcs.reset.is_popup_active());
        assert!(!model.modal.is_active(Context::VcsReset));
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Soft reset to abcd"
        );
    }

    #[test]
    fn selected_remote_ref_is_submitted_and_stale_completion_is_ignored() {
        let mut model = model();
        let revision = start(&mut model);
        targets(&mut model, revision);
        handle_command(&mut model, CommandId("vcs_reset_next"));
        handle_command(&mut model, CommandId("vcs_reset_submit"));
        assert_eq!(model.vcs.reset.target(), Some("origin/main"));

        let revision = model.vcs.reset.revision;
        let stale = Box::new(ResetCompletion {
            repository: PathBuf::from("/workspace"),
            revision: revision.wrapping_sub(1),
            result: Ok("old".to_string()),
        })
        .update(&mut model);
        assert!(stale.is_none());
        assert!(model.vcs.reset.is_resetting());

        Box::new(ResetCompletion {
            repository: PathBuf::from("/workspace"),
            revision,
            result: Err("rejected".to_string()),
        })
        .update(&mut model);
        assert!(model.vcs.reset.is_selecting());
        assert_eq!(model.vcs.reset.error(), Some("rejected"));
    }
}
