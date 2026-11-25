use std::path::{Path, PathBuf};

use crate::{
    component::text_input::{TextInput, TextInputChange, TextInputEvent},
    data_object::{CommandId, Context, Message, Model, StatusMessage},
    manager::{
        command_registry::Command,
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
};

use super::{ChangesState, report, repository_mutation_completed, repository_target};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum MergePhase {
    #[default]
    Closed,
    Loading,
    Selecting,
    Merging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MergeCompletionOutcome {
    Ignored,
    Succeeded,
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct MergeState {
    phase: MergePhase,
    query: TextInput,
    targets: Vec<String>,
    selected: Option<usize>,
    error: Option<String>,
    repository: Option<PathBuf>,
    revision: u64,
    targets_revision: u64,
}

impl MergeState {
    pub(crate) fn is_active(&self) -> bool {
        self.phase != MergePhase::Closed
    }

    pub(crate) fn is_loading(&self) -> bool {
        self.phase == MergePhase::Loading
    }

    pub(crate) fn is_merging(&self) -> bool {
        self.phase == MergePhase::Merging
    }

    pub(crate) fn is_selecting(&self) -> bool {
        self.phase == MergePhase::Selecting
    }

    pub(crate) fn query_state(&self) -> &TextInput {
        &self.query
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn start(&mut self, repository: PathBuf) -> Option<u64> {
        if self.is_merging() {
            return None;
        }
        self.phase = MergePhase::Loading;
        self.query.clear();
        self.targets.clear();
        self.selected = None;
        self.error = None;
        self.repository = Some(repository);
        self.targets_revision = self.targets_revision.wrapping_add(1);
        Some(self.targets_revision)
    }

    fn cancel(&mut self) -> bool {
        if self.is_merging() {
            return false;
        }
        self.phase = MergePhase::Closed;
        self.query.clear();
        self.targets.clear();
        self.selected = None;
        self.error = None;
        true
    }

    fn handle_event(&mut self, event: TextInputEvent) -> Option<TextInputChange> {
        self.is_selecting().then(|| {
            let change = self.query.handle_event(event);
            if change.content_changed() {
                self.select_first();
            }
            change
        })
    }

    fn select(&mut self, next: bool) {
        let len = self.filtered_targets().len();
        if len == 0 || !self.is_selecting() {
            return;
        }
        let selected = self.selected.unwrap_or(0);
        self.selected = Some(match next {
            true => (selected + 1).min(len - 1),
            false => selected.saturating_sub(1),
        });
    }

    fn submit(&mut self, repository: PathBuf) -> Option<(String, u64)> {
        if !self.is_selecting() {
            return None;
        }
        let target = self.selected_target()?;
        self.phase = MergePhase::Merging;
        self.error = None;
        self.repository = Some(repository);
        self.revision = self.revision.wrapping_add(1);
        Some((target, self.revision))
    }

    fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub(crate) fn filtered_targets(&self) -> Vec<&str> {
        let query = self.query.to_lowercase();
        self.targets
            .iter()
            .filter(|target| target.to_lowercase().contains(&query))
            .map(String::as_str)
            .collect()
    }

    fn selected_target(&self) -> Option<String> {
        self.filtered_targets()
            .get(self.selected?)
            .map(|target| target.to_string())
    }

    fn select_first(&mut self) {
        self.selected = (!self.filtered_targets().is_empty()).then_some(0);
    }

    fn complete_targets(
        &mut self,
        repository: PathBuf,
        revision: u64,
        head: Option<&str>,
        result: Result<Vec<String>, String>,
    ) {
        if !self.is_loading()
            || self.targets_revision != revision
            || self.repository.as_ref() != Some(&repository)
        {
            return;
        }
        self.phase = MergePhase::Selecting;
        match result {
            Ok(targets) => {
                self.targets = targets
                    .into_iter()
                    .filter(|target| Some(target.as_str()) != head)
                    .collect();
                self.select_first();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn complete_merge(
        &mut self,
        repository: &Path,
        revision: u64,
        result: Result<(), String>,
    ) -> MergeCompletionOutcome {
        if !self.is_merging()
            || self.revision != revision
            || self.repository.as_deref() != Some(repository)
        {
            return MergeCompletionOutcome::Ignored;
        }
        match result {
            Ok(()) => {
                self.phase = MergePhase::Closed;
                self.query.clear();
                MergeCompletionOutcome::Succeeded
            }
            Err(error) => {
                self.phase = MergePhase::Selecting;
                self.error = Some(error);
                MergeCompletionOutcome::Failed
            }
        }
    }
}

pub(super) fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("vcs_merge_start"),
            "fast-forward merge",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_merge_cancel"),
            "cancel fast-forward merge",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_merge_submit"),
            "submit fast-forward merge",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_merge_next"),
            "select next merge target",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_merge_previous"),
            "select previous merge target",
            handle_command,
        ),
    ]
    .into_iter()
    .collect()
}

pub(super) fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "vcs_merge_start" => start(model),
        "vcs_merge_cancel" => cancel(model),
        "vcs_merge_submit" => submit(model),
        "vcs_merge_next" => select(model, true),
        "vcs_merge_previous" => select(model, false),
        _ => None,
    }
}

fn start(model: &mut Model) -> Option<Effect> {
    if model.vcs.merge.is_merging() {
        return None;
    }
    if !status_is_clean(model) {
        return Some(report(
            model,
            StatusMessage::info("Git status must be clean before merge"),
        ));
    }
    let repository = repository_target(model)?;
    if !model.modal.open(Context::VcsMerge) {
        return None;
    }
    let revision = model.vcs.merge.start(repository.clone())?;
    model.vcs.help_visible = false;
    Some(Effect::feature(MergeTargetsRequest {
        repository,
        revision,
    }))
}

fn cancel(model: &mut Model) -> Option<Effect> {
    if !model.vcs.merge.cancel() {
        return None;
    }
    close_merge_context(model);
    None
}

fn close_merge_context(model: &mut Model) {
    model.modal.close(Context::VcsMerge);
}

pub(crate) fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    model.vcs.merge.handle_event(event);
    None
}

fn select(model: &mut Model, next: bool) -> Option<Effect> {
    model.vcs.merge.select(next);
    None
}

fn submit(model: &mut Model) -> Option<Effect> {
    if !model.vcs.merge.is_selecting() {
        return None;
    }
    if !status_is_clean(model) {
        model
            .vcs
            .merge
            .set_error("Git status must be clean before merge".to_string());
        return None;
    }
    let repository = repository_target(model)?;
    let (target, revision) = model.vcs.merge.submit(repository.clone())?;
    Some(Effect::feature(MergeRequest {
        repository,
        target,
        revision,
    }))
}

fn status_is_clean(model: &Model) -> bool {
    let ChangesState::Ready(changes) = model.vcs.changes() else {
        return false;
    };
    changes.is_empty() && !model.editor.has_modified_buffers() && !model.editor.has_saving_buffers()
}

struct MergeTargetsRequest {
    repository: PathBuf,
    revision: u64,
}

impl FeatureEffect for MergeTargetsRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::local_branches(&self.repository).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(MergeTargetsCompletion {
            repository: self.repository,
            revision: self.revision,
            result,
        }))
    }
}

struct MergeTargetsCompletion {
    repository: PathBuf,
    revision: u64,
    result: Result<Vec<String>, String>,
}

impl Completion for MergeTargetsCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let head = model.vcs.head().map(str::to_string);
        model.vcs.merge.complete_targets(
            self.repository,
            self.revision,
            head.as_deref(),
            self.result,
        );
        None
    }
}

struct MergeRequest {
    repository: PathBuf,
    target: String,
    revision: u64,
}

impl FeatureEffect for MergeRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::fast_forward(&self.repository, &self.target)
            .map(|outcome| match outcome {
                ::vcs::FastForwardOutcome::Updated(id) => Some(id.to_hex_with_len(7).to_string()),
                ::vcs::FastForwardOutcome::AlreadyUpToDate => None,
            })
            .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(MergeCompletion {
            repository: self.repository,
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct MergeCompletion {
    repository: PathBuf,
    target: String,
    revision: u64,
    result: Result<Option<String>, String>,
}

impl Completion for MergeCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let MergeCompletion {
            repository,
            target,
            revision,
            result,
        } = *self;
        let updated = match result {
            Ok(updated) => match model
                .vcs
                .merge
                .complete_merge(&repository, revision, Ok(()))
            {
                MergeCompletionOutcome::Succeeded => updated,
                MergeCompletionOutcome::Ignored | MergeCompletionOutcome::Failed => return None,
            },
            Err(error) => {
                model
                    .vcs
                    .merge
                    .complete_merge(&repository, revision, Err(error));
                return None;
            }
        };

        close_merge_context(model);
        let status = StatusMessage::info(match updated {
            Some(id) => format!("Switched to {target} and fast-forwarded to {id}"),
            None => format!("Switched to {target}; already up to date"),
        });
        Effect::combine(
            repository_mutation_completed(model),
            Some(report(model, status)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{AppPage, Workspace},
        test_utils::create_buffer,
        vcs::HistoryState,
    };
    use ::vcs::{ChangeKind, FileChange};

    fn model(root: PathBuf) -> Model {
        let mut model = Model {
            active_page: AppPage::Vcs,
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.active_page = AppPage::Vcs;
        model.vcs.changes.state = ChangesState::Ready(Vec::new());
        model
    }

    fn open_merge(model: &mut Model) {
        assert!(matches!(start(model), Some(Effect::Feature(_))));
        assert!(model.vcs.merge.is_active());
        assert!(model.vcs.merge.is_loading());
    }

    fn complete_targets(model: &mut Model, targets: &[&str]) {
        Box::new(MergeTargetsCompletion {
            repository: model.vcs.merge.repository.clone().unwrap(),
            revision: model.vcs.merge.targets_revision,
            result: Ok(targets.iter().map(|target| target.to_string()).collect()),
        })
        .update(model);
        assert!(model.vcs.merge.is_selecting());
    }

    #[test]
    fn completion_identity_is_enforced_inside_merge_state() {
        let root = PathBuf::from("/workspace");
        let mut state = MergeState {
            phase: MergePhase::Merging,
            repository: Some(root.clone()),
            revision: 4,
            query: TextInput::new("side"),
            ..MergeState::default()
        };

        assert_eq!(
            state.complete_merge(Path::new("/other"), 4, Ok(())),
            MergeCompletionOutcome::Ignored
        );
        assert_eq!(
            state.complete_merge(&root, 3, Ok(())),
            MergeCompletionOutcome::Ignored
        );
        assert!(state.is_merging());

        assert_eq!(
            state.complete_merge(&root, 4, Err("not a fast-forward".to_string())),
            MergeCompletionOutcome::Failed
        );
        assert!(state.is_selecting());
        assert_eq!(state.error.as_deref(), Some("not a fast-forward"));

        state.phase = MergePhase::Merging;
        assert_eq!(
            state.complete_merge(&root, 4, Ok(())),
            MergeCompletionOutcome::Succeeded
        );
        assert!(!state.is_active());
        assert!(state.query.is_empty());
    }

    #[test]
    fn picker_commands_are_local_state_transitions() {
        let root = PathBuf::from("/workspace");
        let mut state = MergeState::default();
        let targets_revision = state.start(root.clone()).unwrap();
        assert!(state.is_loading());
        state.complete_targets(
            root.clone(),
            targets_revision,
            None,
            Ok(vec!["main".to_string(), "other".to_string()]),
        );
        state.handle_event(TextInputEvent::Insert('o'));
        assert_eq!(state.filtered_targets(), ["other"]);

        let (target, revision) = state.submit(root.clone()).unwrap();
        assert_eq!(target, "other");
        assert!(state.is_merging());
        assert!(!state.cancel());
        assert_eq!(
            state.complete_merge(&root, revision, Err("failed".to_string())),
            MergeCompletionOutcome::Failed
        );
        assert!(state.is_selecting());

        assert!(state.cancel());
        assert!(!state.is_active());
        assert!(state.filtered_targets().is_empty());
        assert!(state.query.is_empty());
    }

    #[test]
    fn pending_changes_prevent_opening_the_picker() {
        let mut model = model(PathBuf::from("/workspace"));
        model.vcs.changes.state = ChangesState::Ready(vec![FileChange {
            kind: ChangeKind::Modified,
            staged: false,
            path: PathBuf::from("/workspace/note.txt"),
            previous_path: None,
        }]);

        assert!(start(&mut model).is_some());
        assert!(!model.vcs.merge.is_active());
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Git status must be clean before merge"
        );

        let mut unsaved_model = self::model(PathBuf::from("/workspace"));
        create_buffer(&mut unsaved_model, "unsaved");
        unsaved_model.editor.enter_insert_mode();
        unsaved_model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("!")
            .unwrap();
        unsaved_model.editor.enter_normal_mode();
        assert!(start(&mut unsaved_model).is_some());
        assert!(!unsaved_model.vcs.merge.is_active());
    }

    #[test]
    fn filtering_selects_a_target_and_failure_stays_in_the_popup() {
        let root = PathBuf::from("/workspace");
        let mut model = model(root.clone());
        open_merge(&mut model);
        complete_targets(&mut model, &["main", "side", "origin/main", "other"]);
        type_query(&mut model, "side");
        assert_eq!(model.vcs.merge.filtered_targets(), ["side"]);
        assert!(matches!(submit(&mut model), Some(Effect::Feature(_))));
        assert!(model.vcs.merge.is_merging());

        Box::new(MergeCompletion {
            repository: root,
            target: "side".to_string(),
            revision: model.vcs.merge.revision,
            result: Err("not a fast-forward".to_string()),
        })
        .update(&mut model);

        assert_eq!(model.vcs.merge.error.as_deref(), Some("not a fast-forward"));
        assert!(model.vcs.merge.is_active());
        assert!(model.vcs.merge.is_selecting());
        assert_eq!(model.vcs.merge.selected_target().as_deref(), Some("side"));
    }

    fn type_query(model: &mut Model, query: &str) {
        for character in query.chars() {
            handle_input(model, TextInputEvent::Insert(character));
        }
    }

    #[test]
    fn success_closes_the_picker_and_stale_completion_is_ignored() {
        let root = PathBuf::from("/workspace");
        let mut model = model(root.clone());
        open_merge(&mut model);
        complete_targets(&mut model, &["side", "other"]);
        select(&mut model, true);
        assert_eq!(model.vcs.merge.selected_target().as_deref(), Some("other"));
        assert!(submit(&mut model).is_some());
        assert!(model.vcs.merge.is_merging());
        let stale_revision = model.vcs.merge.revision.wrapping_sub(1);
        Box::new(MergeCompletion {
            repository: root.clone(),
            target: "other".to_string(),
            revision: stale_revision,
            result: Ok(Some("stale".to_string())),
        })
        .update(&mut model);
        assert!(model.vcs.merge.is_active());
        assert!(model.vcs.merge.is_merging());

        Box::new(MergeCompletion {
            repository: root,
            target: "other".to_string(),
            revision: model.vcs.merge.revision,
            result: Ok(Some("abc1234".to_string())),
        })
        .update(&mut model);
        assert!(!model.vcs.merge.is_active());
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Switched to other and fast-forwarded to abc1234"
        );
        assert_eq!(model.vcs.history.state, HistoryState::Loading);
    }
}
