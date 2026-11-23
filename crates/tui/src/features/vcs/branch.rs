use std::path::PathBuf;

use ::vcs::{BranchStartPoint, BranchSwitchOutcome};

use crate::{
    component::text_input::{TextInput, TextInputChange, TextInputEvent},
    data_object::{CommandId, Context, Message, Model, StatusMessage},
    manager::{
        command_registry::Command,
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
};

use super::{report, repository_mutation_completed, repository_target};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum BranchPhase {
    #[default]
    Closed,
    Menu,
    Loading,
    Selecting,
    Naming,
    Switching,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchMode {
    Switch,
    Create,
}

#[derive(Debug, Default)]
pub(crate) struct BranchState {
    phase: BranchPhase,
    mode: Option<BranchMode>,
    query: TextInput,
    branches: Vec<String>,
    selected: Option<usize>,
    start: Option<BranchStartPoint>,
    error: Option<String>,
    repository: Option<PathBuf>,
    targets_revision: u64,
    switch_revision: u64,
}

impl BranchState {
    pub(crate) fn is_menu(&self) -> bool {
        self.phase == BranchPhase::Menu
    }

    pub(crate) fn is_popup_active(&self) -> bool {
        [
            BranchPhase::Loading,
            BranchPhase::Selecting,
            BranchPhase::Naming,
            BranchPhase::Switching,
        ]
        .contains(&self.phase)
    }

    pub(crate) fn is_loading(&self) -> bool {
        self.phase == BranchPhase::Loading
    }

    pub(crate) fn is_switching(&self) -> bool {
        self.phase == BranchPhase::Switching
    }

    pub(crate) fn is_naming(&self) -> bool {
        self.phase == BranchPhase::Naming
    }

    pub(crate) fn is_selecting(&self) -> bool {
        self.phase == BranchPhase::Selecting
    }

    pub(crate) fn is_creating(&self) -> bool {
        self.mode == Some(BranchMode::Create)
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

    pub(crate) fn revision_query(&self) -> Option<&str> {
        if !self.is_creating() || !self.is_selecting() {
            return None;
        }
        let query = self.query.trim();
        (query.len() >= 4 && query.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(query)
    }

    pub(crate) fn filtered_targets(&self) -> Vec<&str> {
        let query = self.query.to_lowercase();
        let mut targets = Vec::new();
        if self.is_creating() && "head".contains(&query) {
            targets.push("HEAD");
        }
        targets.extend(
            self.branches
                .iter()
                .filter(|branch| branch.to_lowercase().contains(&query))
                .map(String::as_str),
        );
        targets
    }

    pub(crate) fn start_description(&self) -> Option<String> {
        self.start.as_ref().map(|start| match start {
            BranchStartPoint::Head => "From HEAD".to_string(),
            BranchStartPoint::Branch(branch) => format!("From branch {branch}"),
            BranchStartPoint::Revision(revision) => format!("From commit {revision}"),
        })
    }

    fn open_menu(&mut self) -> bool {
        if self.phase != BranchPhase::Closed {
            return false;
        }
        self.phase = BranchPhase::Menu;
        self.error = None;
        true
    }

    fn start(&mut self, repository: PathBuf, mode: BranchMode) -> Option<u64> {
        if self.phase != BranchPhase::Menu {
            return None;
        }
        self.phase = BranchPhase::Loading;
        self.mode = Some(mode);
        self.query.clear();
        self.branches.clear();
        self.selected = None;
        self.start = None;
        self.error = None;
        self.repository = Some(repository);
        self.targets_revision = self.targets_revision.wrapping_add(1);
        Some(self.targets_revision)
    }

    fn cancel(&mut self) -> bool {
        if self.phase == BranchPhase::Switching {
            return false;
        }
        let active = self.phase != BranchPhase::Closed;
        self.phase = BranchPhase::Closed;
        self.mode = None;
        self.query.clear();
        self.branches.clear();
        self.selected = None;
        self.start = None;
        self.error = None;
        active
    }

    fn handle_event(&mut self, event: TextInputEvent) -> Option<TextInputChange> {
        [BranchPhase::Selecting, BranchPhase::Naming]
            .contains(&self.phase)
            .then(|| {
                let change = self.query.handle_event(event);
                if change.content_changed() {
                    self.error = None;
                    if self.phase == BranchPhase::Selecting {
                        self.select_first();
                    }
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

    fn submit(&mut self, repository: PathBuf) -> Option<BranchSubmission> {
        match self.phase {
            BranchPhase::Selecting => match self.mode? {
                BranchMode::Switch => {
                    let branch = self.selected_target()?;
                    self.phase = BranchPhase::Switching;
                    self.error = None;
                    self.repository = Some(repository);
                    self.switch_revision = self.switch_revision.wrapping_add(1);
                    Some(BranchSubmission::Switch {
                        operation: BranchOperation::Existing(branch),
                        revision: self.switch_revision,
                    })
                }
                BranchMode::Create => {
                    let start = self.revision_query().map_or_else(
                        || self.selected_start_point(),
                        |revision| Some(BranchStartPoint::Revision(revision.to_string())),
                    )?;
                    self.start = Some(start);
                    self.phase = BranchPhase::Naming;
                    self.query.clear();
                    self.selected = None;
                    self.error = None;
                    Some(BranchSubmission::Naming)
                }
            },
            BranchPhase::Naming => {
                let name = self.query.trim().to_string();
                if name.is_empty() {
                    self.error = Some("Branch name is required".to_string());
                    return None;
                }
                let start = self.start.clone()?;
                self.phase = BranchPhase::Switching;
                self.error = None;
                self.repository = Some(repository);
                self.switch_revision = self.switch_revision.wrapping_add(1);
                Some(BranchSubmission::Switch {
                    operation: BranchOperation::Create { name, start },
                    revision: self.switch_revision,
                })
            }
            _ => None,
        }
    }

    fn selected_target(&self) -> Option<String> {
        self.filtered_targets()
            .get(self.selected?)
            .map(|target| target.to_string())
    }

    fn selected_start_point(&self) -> Option<BranchStartPoint> {
        let target = self.selected_target()?;
        Some(if target == "HEAD" {
            BranchStartPoint::Head
        } else {
            BranchStartPoint::Branch(target)
        })
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
        if self.phase != BranchPhase::Loading
            || self.targets_revision != revision
            || self.repository.as_ref() != Some(&repository)
        {
            return;
        }
        self.phase = BranchPhase::Selecting;
        match result {
            Ok(branches) => {
                self.branches = branches
                    .into_iter()
                    .filter(|branch| self.is_creating() || Some(branch.as_str()) != head)
                    .collect();
                self.select_first();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn complete_switch(
        &mut self,
        repository: &PathBuf,
        revision: u64,
        result: Result<(), String>,
    ) -> BranchCompletionOutcome {
        if self.phase != BranchPhase::Switching
            || self.switch_revision != revision
            || self.repository.as_ref() != Some(repository)
        {
            return BranchCompletionOutcome::Ignored;
        }
        match result {
            Ok(()) => {
                self.phase = BranchPhase::Closed;
                self.mode = None;
                self.query.clear();
                self.branches.clear();
                self.selected = None;
                self.start = None;
                self.error = None;
                BranchCompletionOutcome::Succeeded
            }
            Err(error) => {
                self.phase = if self.is_creating() {
                    BranchPhase::Naming
                } else {
                    BranchPhase::Selecting
                };
                self.error = Some(error);
                BranchCompletionOutcome::Failed
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchCompletionOutcome {
    Ignored,
    Succeeded,
    Failed,
}

enum BranchSubmission {
    Naming,
    Switch {
        operation: BranchOperation,
        revision: u64,
    },
}

enum BranchOperation {
    Existing(String),
    Create {
        name: String,
        start: BranchStartPoint,
    },
}

pub(super) fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("vcs_branch_menu"),
            "branch actions",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_branch_menu_close"),
            "close branch actions",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_branch_create"),
            "create and switch branch",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_branch_switch"),
            "switch existing branch",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_branch_cancel"),
            "cancel branch workflow",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_branch_submit"),
            "accept branch selection or name",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_branch_next"),
            "select next branch",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_branch_previous"),
            "select previous branch",
            handle_command,
        ),
    ]
    .into_iter()
    .collect()
}

pub(super) fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "vcs_branch_menu" => open_menu(model),
        "vcs_branch_menu_close" => close_menu(model),
        "vcs_branch_create" => start(model, BranchMode::Create),
        "vcs_branch_switch" => start(model, BranchMode::Switch),
        "vcs_branch_cancel" => cancel(model),
        "vcs_branch_submit" => submit(model),
        "vcs_branch_next" => select(model, true),
        "vcs_branch_previous" => select(model, false),
        _ => None,
    }
}

fn open_menu(model: &mut Model) -> Option<Effect> {
    if !model.vcs.branch.open_menu() {
        return None;
    }
    model.vcs.help_visible = false;
    model.context_stack.push(Context::VcsBranchMenu);
    None
}

fn close_menu(model: &mut Model) -> Option<Effect> {
    if !model.vcs.branch.cancel() {
        return None;
    }
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsBranchMenu);
    None
}

fn start(model: &mut Model, mode: BranchMode) -> Option<Effect> {
    let repository = repository_target(model)?;
    if !model.modal.open(Context::VcsBranch) {
        return None;
    }
    let Some(revision) = model.vcs.branch.start(repository.clone(), mode) else {
        model.modal.close(Context::VcsBranch);
        return None;
    };
    model.vcs.help_visible = false;
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsBranchMenu);
    model.context_stack.push(Context::VcsBranch);
    Some(Effect::feature(BranchTargetsRequest {
        repository,
        revision,
    }))
}

fn cancel(model: &mut Model) -> Option<Effect> {
    if !model.vcs.branch.cancel() {
        return None;
    }
    close_branch_context(model);
    None
}

fn close_branch_context(model: &mut Model) {
    model.modal.close(Context::VcsBranch);
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsBranch);
}

pub(crate) fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    model.vcs.branch.handle_event(event);
    None
}

fn select(model: &mut Model, next: bool) -> Option<Effect> {
    model.vcs.branch.select(next);
    None
}

fn submit(model: &mut Model) -> Option<Effect> {
    let repository = repository_target(model)?;
    match model.vcs.branch.submit(repository.clone())? {
        BranchSubmission::Naming => None,
        BranchSubmission::Switch {
            operation,
            revision,
        } => Some(Effect::feature(BranchRequest {
            repository,
            operation,
            revision,
        })),
    }
}

struct BranchTargetsRequest {
    repository: PathBuf,
    revision: u64,
}

impl FeatureEffect for BranchTargetsRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::local_branches(&self.repository).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(BranchTargetsCompletion {
            repository: self.repository,
            revision: self.revision,
            result,
        }))
    }
}

struct BranchTargetsCompletion {
    repository: PathBuf,
    revision: u64,
    result: Result<Vec<String>, String>,
}

impl Completion for BranchTargetsCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let head = model.vcs.head().map(str::to_string);
        model.vcs.branch.complete_targets(
            self.repository,
            self.revision,
            head.as_deref(),
            self.result,
        );
        None
    }
}

struct BranchRequest {
    repository: PathBuf,
    operation: BranchOperation,
    revision: u64,
}

impl FeatureEffect for BranchRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = match self.operation {
            BranchOperation::Existing(branch) => ::vcs::switch_branch(&self.repository, &branch),
            BranchOperation::Create { name, start } => {
                ::vcs::create_branch_and_switch(&self.repository, &name, start)
            }
        }
        .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(BranchCompletion {
            repository: self.repository,
            revision: self.revision,
            result,
        }))
    }
}

struct BranchCompletion {
    repository: PathBuf,
    revision: u64,
    result: Result<BranchSwitchOutcome, String>,
}

impl Completion for BranchCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let BranchCompletion {
            repository,
            revision,
            result,
        } = *self;
        let outcome = match result {
            Ok(outcome) => {
                match model
                    .vcs
                    .branch
                    .complete_switch(&repository, revision, Ok(()))
                {
                    BranchCompletionOutcome::Succeeded => outcome,
                    BranchCompletionOutcome::Ignored | BranchCompletionOutcome::Failed => {
                        return None;
                    }
                }
            }
            Err(error) => {
                model
                    .vcs
                    .branch
                    .complete_switch(&repository, revision, Err(error));
                return None;
            }
        };

        close_branch_context(model);
        let id = outcome.commit.to_hex_with_len(7);
        let status = StatusMessage::info(if outcome.created {
            format!("Created and switched to {} at {id}", outcome.branch)
        } else {
            format!("Switched to {}", outcome.branch)
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
    use crate::data_object::{AppPage, Workspace};

    fn repository_path() -> PathBuf {
        PathBuf::from("/workspace")
    }

    fn loading_state(mode: BranchMode) -> BranchState {
        let mut state = BranchState::default();
        assert!(state.open_menu());
        state.start(repository_path(), mode).unwrap();
        state
    }

    #[test]
    fn create_flow_orders_start_points_and_accepts_a_revision_and_name() {
        let mut state = loading_state(BranchMode::Create);
        state.complete_targets(
            repository_path(),
            1,
            Some("main"),
            Ok(vec!["main".to_string(), "side".to_string()]),
        );
        assert_eq!(state.filtered_targets(), ["HEAD", "main", "side"]);
        assert_eq!(state.selected, Some(0));

        for character in "abc123".chars() {
            state.handle_event(TextInputEvent::Insert(character));
        }
        assert_eq!(state.revision_query(), Some("abc123"));
        assert!(matches!(
            state.submit(repository_path()),
            Some(BranchSubmission::Naming)
        ));
        assert!(state.is_naming());
        assert_eq!(
            state.start,
            Some(BranchStartPoint::Revision("abc123".to_string()))
        );
        for character in "topic/test".chars() {
            state.handle_event(TextInputEvent::Insert(character));
        }

        let Some(BranchSubmission::Switch {
            operation: BranchOperation::Create { name, start },
            revision,
        }) = state.submit(repository_path())
        else {
            panic!("expected create-and-switch submission");
        };
        assert_eq!(name, "topic/test");
        assert_eq!(start, BranchStartPoint::Revision("abc123".to_string()));
        assert!(state.is_switching());

        assert_eq!(
            state.complete_switch(
                &repository_path(),
                revision,
                Err("local changes would be overwritten".to_string()),
            ),
            BranchCompletionOutcome::Failed
        );
        assert!(state.is_naming());
        assert_eq!(state.query.value(), "topic/test");
        assert!(state.error().unwrap().contains("local changes"));
    }

    #[test]
    fn head_and_branch_rows_advance_to_naming_with_the_typed_start() {
        let mut head = loading_state(BranchMode::Create);
        head.complete_targets(repository_path(), 1, None, Ok(vec!["main".to_string()]));
        assert!(matches!(
            head.submit(repository_path()),
            Some(BranchSubmission::Naming)
        ));
        assert_eq!(head.start, Some(BranchStartPoint::Head));

        let mut branch = loading_state(BranchMode::Create);
        branch.complete_targets(
            repository_path(),
            1,
            None,
            Ok(vec!["main".to_string(), "side".to_string()]),
        );
        branch.select(true);
        branch.select(true);
        assert_eq!(branch.selected, Some(2));
        assert!(matches!(
            branch.submit(repository_path()),
            Some(BranchSubmission::Naming)
        ));
        assert_eq!(
            branch.start,
            Some(BranchStartPoint::Branch("side".to_string()))
        );
    }

    #[test]
    fn switch_flow_filters_head_and_enforces_completion_identity() {
        let mut state = loading_state(BranchMode::Switch);
        state.complete_targets(
            repository_path(),
            1,
            Some("main"),
            Ok(vec!["main".to_string(), "side".to_string()]),
        );
        assert_eq!(state.filtered_targets(), ["side"]);
        let Some(BranchSubmission::Switch {
            operation: BranchOperation::Existing(branch),
            revision,
        }) = state.submit(repository_path())
        else {
            panic!("expected existing-branch submission");
        };
        assert_eq!(branch, "side");

        assert_eq!(
            state.complete_switch(&repository_path(), revision.wrapping_add(1), Ok(())),
            BranchCompletionOutcome::Ignored
        );
        assert!(state.is_switching());
        assert_eq!(
            state.complete_switch(&repository_path(), revision, Err("unsafe".to_string())),
            BranchCompletionOutcome::Failed
        );
        assert!(state.is_selecting());
        assert_eq!(state.error(), Some("unsafe"));

        let Some(BranchSubmission::Switch { revision, .. }) = state.submit(repository_path())
        else {
            panic!("expected retry");
        };
        assert_eq!(
            state.complete_switch(&repository_path(), revision, Ok(())),
            BranchCompletionOutcome::Succeeded
        );
        assert!(!state.is_popup_active());
    }

    #[test]
    fn cancelled_loading_ignores_its_branch_list() {
        let mut state = loading_state(BranchMode::Switch);
        assert!(state.cancel());
        state.complete_targets(repository_path(), 1, None, Ok(vec!["side".to_string()]));
        assert!(!state.is_popup_active());
        assert!(state.branches.is_empty());
    }

    #[test]
    fn competing_modal_preserves_the_branch_menu() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = Model {
            active_page: AppPage::Vcs,
            workspace: Some(Workspace::new(directory.path().to_path_buf())),
            ..Model::default()
        };
        model.active_page = AppPage::Vcs;
        open_menu(&mut model);
        assert!(model.vcs.branch.is_menu());
        assert_eq!(model.context_stack.last(), Some(&Context::VcsBranchMenu));
        assert!(model.modal.open(Context::BufferGrep));

        assert!(start(&mut model, BranchMode::Create).is_none());
        assert!(model.vcs.branch.is_menu());
        assert_eq!(model.context_stack.last(), Some(&Context::VcsBranchMenu));

        model.modal.close(Context::BufferGrep);
        let effect = start(&mut model, BranchMode::Create);
        assert!(matches!(effect, Some(Effect::Feature(_))));
        assert!(model.modal.is_active(Context::VcsBranch));
        assert_eq!(model.context_stack.last(), Some(&Context::VcsBranch));
        assert!(model.vcs.branch.is_popup_active());

        cancel(&mut model);
        assert!(!model.modal.is_active(Context::VcsBranch));
        assert!(model.context_stack.is_empty());
    }
}
