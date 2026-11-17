use std::path::{Path, PathBuf};

use crate::{
    data_object::{CommandId, Context, Message, Model},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        text_input_registry::TextInputRegistry,
    },
};

use super::{ChangesState, repository_changed, repository_target};

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
    pub(crate) query: String,
    pub(crate) targets: Vec<String>,
    pub(crate) selected: Option<usize>,
    pub(crate) error: Option<String>,
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

    fn is_selecting(&self) -> bool {
        self.phase == MergePhase::Selecting
    }

    #[cfg(test)]
    pub(crate) fn show_for_test(&mut self) {
        self.phase = MergePhase::Selecting;
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

pub(super) fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
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
            CommandId("vcs_merge_backspace"),
            "delete merge filter character",
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
    .try_for_each(|command| commands.register(command))
}

pub(crate) fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::VcsMerge, insert_char)
}

pub(super) fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "vcs_merge_start" => start(model),
        "vcs_merge_cancel" => cancel(model),
        "vcs_merge_submit" => submit(model),
        "vcs_merge_backspace" => backspace(model),
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
        model.vcs.notice = Some("Git status must be clean before merge".to_string());
        return None;
    }
    let repository = repository_target(model)?;
    model.vcs.help_visible = false;
    model.vcs.merge.phase = MergePhase::Loading;
    model.vcs.merge.query.clear();
    model.vcs.merge.targets.clear();
    model.vcs.merge.selected = None;
    model.vcs.merge.error = None;
    model.vcs.merge.repository = Some(repository.clone());
    model.vcs.merge.targets_revision = model.vcs.merge.targets_revision.wrapping_add(1);
    model.context_stack.push(Context::VcsMerge);
    Some(Effect::feature(MergeTargetsRequest {
        repository,
        revision: model.vcs.merge.targets_revision,
    }))
}

fn cancel(model: &mut Model) -> Option<Effect> {
    if model.vcs.merge.is_merging() {
        return None;
    }
    close_prompt(model);
    model.vcs.merge.query.clear();
    model.vcs.merge.targets.clear();
    model.vcs.merge.selected = None;
    model.vcs.merge.error = None;
    None
}

fn close_prompt(model: &mut Model) {
    model.vcs.merge.phase = MergePhase::Closed;
    close_merge_context(model);
}

fn close_merge_context(model: &mut Model) {
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsMerge);
}

fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    if model.vcs.merge.is_selecting() {
        model.vcs.merge.query.push(character);
        model.vcs.merge.select_first();
    }
    None
}

fn backspace(model: &mut Model) -> Option<Effect> {
    if model.vcs.merge.is_selecting() {
        model.vcs.merge.query.pop();
        model.vcs.merge.select_first();
    }
    None
}

fn select(model: &mut Model, next: bool) -> Option<Effect> {
    let len = model.vcs.merge.filtered_targets().len();
    if len == 0 || !model.vcs.merge.is_selecting() {
        return None;
    }
    let selected = model.vcs.merge.selected.unwrap_or(0);
    model.vcs.merge.selected = Some(match next {
        true => (selected + 1).min(len - 1),
        false => selected.saturating_sub(1),
    });
    None
}

fn submit(model: &mut Model) -> Option<Effect> {
    if !model.vcs.merge.is_selecting() {
        return None;
    }
    if !status_is_clean(model) {
        model.vcs.merge.error = Some("Git status must be clean before merge".to_string());
        return None;
    }
    let target = model.vcs.merge.selected_target()?;
    let repository = repository_target(model)?;
    model.vcs.merge.phase = MergePhase::Merging;
    model.vcs.merge.error = None;
    model.vcs.merge.repository = Some(repository.clone());
    model.vcs.merge.revision = model.vcs.merge.revision.wrapping_add(1);
    Some(Effect::feature(MergeRequest {
        repository,
        target,
        revision: model.vcs.merge.revision,
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
        let result = ::vcs::merge_targets(&self.repository).map_err(|error| error.to_string());
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
        model.vcs.notice = Some(match updated {
            Some(id) => format!("Switched to {target} and fast-forwarded to {id}"),
            None => format!("Switched to {target}; already up to date"),
        });
        let effect = repository_changed(model);
        if let Some(root) = model
            .workspace
            .as_ref()
            .map(|workspace| workspace.root().to_path_buf())
        {
            crate::explorer::refresh_changed(model, &root);
        }
        Effect::combine(effect, crate::external_sync::reload_open_buffers(model))
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
        model.context_stack.push(Context::VCS);
        model.vcs.changes = ChangesState::Ready(Vec::new());
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
            query: "side".to_string(),
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
    fn pending_changes_prevent_opening_the_picker() {
        let mut model = model(PathBuf::from("/workspace"));
        model.vcs.changes = ChangesState::Ready(vec![FileChange {
            kind: ChangeKind::Modified,
            staged: false,
            path: PathBuf::from("/workspace/note.txt"),
            previous_path: None,
        }]);

        assert!(start(&mut model).is_none());
        assert!(!model.vcs.merge.is_active());
        assert_eq!(
            model.vcs.notice.as_deref(),
            Some("Git status must be clean before merge")
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
        assert!(start(&mut unsaved_model).is_none());
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
            insert_char(model, character);
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
        assert_eq!(
            model.vcs.notice.as_deref(),
            Some("Switched to other and fast-forwarded to abc1234")
        );
        assert_eq!(model.vcs.history, HistoryState::Loading);
    }
}
