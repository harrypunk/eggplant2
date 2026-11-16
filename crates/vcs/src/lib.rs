use std::path::Path;

use anyhow::Context as _;
use gix::{
    Repository,
    bstr::{BStr, BString, ByteSlice as _},
    diff::{
        blob::unified_diff::{ConsumeBinaryHunk, ContextSize},
        blob::{Algorithm, InternedInput, UnifiedDiff},
        index::{
            Change as TreeIndexChange,
            ChangeRef::{
                Addition, Deletion, Modification as TreeModification, Rewrite as TreeRewrite,
            },
        },
    },
    dir::entry::Status as DirEntryStatus,
    hash::ObjectId,
    index::{
        File as IndexFile, State as IndexState,
        entry::{Flags, Mode, Stage, Stat},
        fs::Metadata as IndexMetadata,
    },
    object::tree::{EntryKind, EntryMode, diff::ChangeDetached},
    sec::trust::DefaultForLevel as _,
    status::{
        Item as StatusItem,
        index_worktree::{
            Item as IndexWorktreeItem,
            Item::{
                DirectoryContents, Modification as WorktreeModification, Rewrite as WorktreeRewrite,
            },
        },
        iter::Error as StatusIterError,
        plumbing::index_as_worktree::{Change, EntryStatus},
    },
};
use helix_vcs::DiffProviderRegistry;

/// Return the current Git branch name or abbreviated detached commit.
///
/// Non-repositories, unborn HEADs, relative paths, and discovery failures have
/// no displayable head and return `None`. Repository-local configuration is
/// opened with reduced trust.
pub fn current_head(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    let directory_probe = path.is_dir().then(|| path.join(".eggplant-vcs-head"));
    let path = directory_probe.as_deref().unwrap_or(path);
    DiffProviderRegistry::default()
        .get_current_head_name(path, false)
        .map(|head| head.load().to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum DiffBaseError {
    #[error("not a Git repository")]
    NotRepository,
}

/// Load a file's HEAD content. Untracked files return `None` for an empty base.
pub fn diff_base(path: &Path) -> Result<Option<Vec<u8>>, DiffBaseError> {
    open_repository(path).map_err(|_| DiffBaseError::NotRepository)?;
    Ok(DiffProviderRegistry::default().get_diff_base(path, false))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFile {
    pub kind: ChangeKind,
    pub path: String,
    pub previous_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDetails {
    pub full_id: String,
    pub message: String,
    pub author: String,
    pub date: String,
    pub files: Vec<CommitFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub id: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPage {
    pub commits: Vec<CommitSummary>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Untracked,
    Modified,
    Conflict,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub kind: ChangeKind,
    pub staged: bool,
    pub path: std::path::PathBuf,
    pub previous_path: Option<std::path::PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ChangesError {
    #[error("not a Git repository")]
    NotRepository,
    #[error("failed to read changed files: {0}")]
    Read(#[source] anyhow::Error),
}

/// List staged and unstaged files with absolute paths, sorted for deterministic display.
pub fn changes(path: &Path) -> Result<Vec<FileChange>, ChangesError> {
    let repository = open_repository(path).map_err(|_| ChangesError::NotRepository)?;
    let workdir = repository
        .workdir()
        .context("working tree not found")
        .map_err(ChangesError::Read)?
        .to_path_buf();
    let status = repository
        .status(gix::progress::Discard)
        .map_err(|error| ChangesError::Read(error.into()))?
        .untracked_files(gix::status::UntrackedFiles::Files);
    let items = status
        .into_iter(Vec::new())
        .map_err(|error| ChangesError::Read(error.into()))?;
    let mut changes = collect_changes(&workdir, items)?;
    changes.sort_by(|left, right| {
        right
            .staged
            .cmp(&left.staged)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(changes)
}

fn collect_changes(
    workdir: &Path,
    mut items: impl Iterator<Item = Result<StatusItem, StatusIterError>>,
) -> Result<Vec<FileChange>, ChangesError> {
    items.try_fold(Vec::new(), |mut changes, item| {
        let item = item.map_err(|error| ChangesError::Read(error.into()))?;
        if let Some(change) = change_from_status(workdir, item) {
            changes.push(change);
        }
        Ok::<_, ChangesError>(changes)
    })
}

fn change_from_status(workdir: &Path, item: StatusItem) -> Option<FileChange> {
    match item {
        StatusItem::IndexWorktree(item) => change_from_index_worktree(workdir, item),
        StatusItem::TreeIndex(change) => change_from_tree_index(workdir, change),
    }
}

fn change_from_index_worktree(workdir: &Path, item: IndexWorktreeItem) -> Option<FileChange> {
    match item {
        WorktreeModification {
            rela_path, status, ..
        } => {
            let kind = match status {
                EntryStatus::Conflict { .. } => ChangeKind::Conflict,
                EntryStatus::Change(Change::Removed) => ChangeKind::Deleted,
                EntryStatus::Change(Change::Modification { .. }) => ChangeKind::Modified,
                EntryStatus::IntentToAdd => ChangeKind::Untracked,
                _ => return None,
            };
            Some(FileChange {
                kind,
                staged: false,
                path: workdir.join(rela_path.to_path().ok()?),
                previous_path: None,
            })
        }
        DirectoryContents { entry, .. } if entry.status == DirEntryStatus::Untracked => {
            Some(FileChange {
                kind: ChangeKind::Untracked,
                staged: false,
                path: workdir.join(entry.rela_path.to_path().ok()?),
                previous_path: None,
            })
        }
        WorktreeRewrite {
            source,
            dirwalk_entry,
            ..
        } => Some(FileChange {
            kind: ChangeKind::Renamed,
            staged: false,
            path: workdir.join(dirwalk_entry.rela_path.to_path().ok()?),
            previous_path: Some(workdir.join(source.rela_path().to_path().ok()?)),
        }),
        _ => None,
    }
}

fn change_from_tree_index(workdir: &Path, change: TreeIndexChange) -> Option<FileChange> {
    match change {
        Addition { location, .. } => Some(FileChange {
            kind: ChangeKind::Added,
            staged: true,
            path: workdir.join(location.to_path().ok()?),
            previous_path: None,
        }),
        Deletion { location, .. } => Some(FileChange {
            kind: ChangeKind::Deleted,
            staged: true,
            path: workdir.join(location.to_path().ok()?),
            previous_path: None,
        }),
        TreeModification { location, .. } => Some(FileChange {
            kind: ChangeKind::Modified,
            staged: true,
            path: workdir.join(location.to_path().ok()?),
            previous_path: None,
        }),
        TreeRewrite {
            source_location,
            location,
            ..
        } => Some(FileChange {
            kind: ChangeKind::Renamed,
            staged: true,
            path: workdir.join(location.to_path().ok()?),
            previous_path: Some(workdir.join(source_location.to_path().ok()?)),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub lines: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("{0}")]
    Failed(String),
}

/// Return a bounded diff for one changed file, including untracked files.
pub fn diff(change: &FileChange, limit: usize) -> Result<Diff, DiffError> {
    if limit == 0 {
        return Ok(Diff {
            lines: Vec::new(),
            truncated: false,
        });
    }
    diff_inner(change, limit).map_err(|error| DiffError::Failed(error.to_string()))
}

fn diff_inner(change: &FileChange, limit: usize) -> anyhow::Result<Diff> {
    let repository = open_repository(&change.path)?;
    let relative = relative_path(&repository, &change.path)?;
    let head_id = head_entry(
        &repository,
        &gix::path::from_bstr(relative.as_slice().as_bstr()),
    )
    .map(|(id, _)| id);
    let indexed = index_entry(&repository, relative.as_slice().as_bstr());
    let (before_id, after_id) = match change.staged {
        true => (head_id, indexed),
        false => (indexed, None),
    };
    let before = read_optional_blob(&repository, before_id)?;
    let after = read_diff_after(&repository, change, after_id)?;
    let output = render_unified_diff(&before, &after)?;
    Ok(bounded_diff_lines(
        String::from_utf8_lossy(&output)
            .lines()
            .map(str::to_string)
            .collect(),
        limit,
    ))
}

fn read_diff_after(
    repository: &Repository,
    change: &FileChange,
    indexed: Option<ObjectId>,
) -> anyhow::Result<Vec<u8>> {
    match (change.staged, change.kind) {
        (true, _) => read_optional_blob(repository, indexed),
        (false, ChangeKind::Deleted) => Ok(Vec::new()),
        (false, _) => Ok(std::fs::read(&change.path)?),
    }
}

fn render_unified_diff(before: &[u8], after: &[u8]) -> anyhow::Result<Vec<u8>> {
    let input = InternedInput::new(before, after);
    let diff = gix::diff::blob::diff_with_slider_heuristics(Algorithm::Histogram, &input);
    Ok(UnifiedDiff::new(
        &diff,
        &input,
        ConsumeBinaryHunk::new(Vec::new(), "\n"),
        ContextSize::default(),
    )
    .consume()?)
}

fn index_entry(repository: &Repository, relative: &BStr) -> Option<ObjectId> {
    repository
        .index_or_empty()
        .ok()?
        .entry_by_path(relative)
        .map(|entry| entry.id)
}

fn read_optional_blob(repository: &Repository, id: Option<ObjectId>) -> anyhow::Result<Vec<u8>> {
    match id {
        Some(id) => Ok(repository.find_object(id)?.detach().data),
        None => Ok(Vec::new()),
    }
}

fn bounded_diff_lines(mut lines: Vec<String>, limit: usize) -> Diff {
    let truncated = lines.len() > limit;
    lines.truncate(limit);
    Diff { lines, truncated }
}

/// Read a commit's first-parent diff as bounded unified text.
pub fn commit_diff(path: &Path, commit_id: &str, limit: usize) -> Result<Diff, DiffError> {
    if limit == 0 {
        return Ok(Diff {
            lines: Vec::new(),
            truncated: false,
        });
    }
    commit_diff_inner(path, commit_id, limit).map_err(|error| DiffError::Failed(error.to_string()))
}

fn commit_diff_inner(path: &Path, commit_id: &str, limit: usize) -> anyhow::Result<Diff> {
    let repository = open_repository(path)?;
    let commit = repository
        .rev_parse_single(commit_id)?
        .object()?
        .into_commit();
    let parent_tree = match commit.parent_ids().next() {
        Some(parent) => Some(parent.object()?.into_commit().tree()?),
        None => None,
    };
    let changes = repository.diff_tree_to_tree(parent_tree.as_ref(), &commit.tree()?, None)?;
    let mut lines = Vec::new();
    for change in changes {
        let Some((before, after, path)) = commit_change_parts(change) else {
            continue;
        };
        let before = read_optional_blob(&repository, before)?;
        let after = read_optional_blob(&repository, after)?;
        lines.push(path);
        lines.extend(
            String::from_utf8_lossy(&render_unified_diff(&before, &after)?)
                .lines()
                .map(str::to_string),
        );
    }
    Ok(bounded_diff_lines(lines, limit))
}

fn commit_change_parts(
    change: gix::object::tree::diff::ChangeDetached,
) -> Option<(Option<ObjectId>, Option<ObjectId>, String)> {
    match change {
        gix::object::tree::diff::ChangeDetached::Addition {
            location,
            id,
            entry_mode,
            ..
        } if is_blob(entry_mode) => Some((None, Some(id), location.to_str_lossy().into_owned())),
        gix::object::tree::diff::ChangeDetached::Deletion {
            location,
            id,
            entry_mode,
            ..
        } if is_blob(entry_mode) => Some((Some(id), None, location.to_str_lossy().into_owned())),
        gix::object::tree::diff::ChangeDetached::Modification {
            location,
            previous_entry_mode,
            entry_mode,
            previous_id,
            id,
            ..
        } if is_blob(previous_entry_mode) && is_blob(entry_mode) => Some((
            Some(previous_id),
            Some(id),
            location.to_str_lossy().into_owned(),
        )),
        gix::object::tree::diff::ChangeDetached::Rewrite {
            location,
            source_entry_mode,
            entry_mode,
            source_id,
            id,
            ..
        } if is_blob(source_entry_mode) && is_blob(entry_mode) => Some((
            Some(source_id),
            Some(id),
            location.to_str_lossy().into_owned(),
        )),
        _ => None,
    }
}

fn is_blob(mode: EntryMode) -> bool {
    matches!(mode.kind(), EntryKind::Blob | EntryKind::BlobExecutable)
}

/// Read complete commit metadata and its changed-file list.
pub fn commit_details(path: &Path, commit_id: &str) -> Result<CommitDetails, DiffError> {
    commit_details_inner(path, commit_id).map_err(|error| DiffError::Failed(error.to_string()))
}

/// Read a unified diff for one file in a commit's first-parent change.
pub fn commit_file_diff(
    path: &Path,
    commit_id: &str,
    file_path: &str,
    limit: usize,
) -> Result<Diff, DiffError> {
    commit_file_diff_inner(path, commit_id, file_path, limit)
        .map_err(|error| DiffError::Failed(error.to_string()))
}

fn commit_details_inner(path: &Path, commit_id: &str) -> anyhow::Result<CommitDetails> {
    let repository = open_repository(path)?;
    let commit = repository
        .rev_parse_single(commit_id)?
        .object()?
        .into_commit();
    let parent_tree = match commit.parent_ids().next() {
        Some(parent) => Some(parent.object()?.into_commit().tree()?),
        None => None,
    };
    let author = commit.author()?;
    let files = repository
        .diff_tree_to_tree(parent_tree.as_ref(), &commit.tree()?, None)?
        .into_iter()
        .filter_map(commit_file)
        .collect();
    Ok(CommitDetails {
        full_id: commit.id.to_string(),
        message: commit.message_raw_sloppy().to_str_lossy().into_owned(),
        author: author.name.to_str_lossy().into_owned(),
        date: author
            .time()?
            .format_or_unix(gix::date::time::format::SHORT),
        files,
    })
}

fn commit_file_diff_inner(
    path: &Path,
    commit_id: &str,
    file_path: &str,
    limit: usize,
) -> anyhow::Result<Diff> {
    let repository = open_repository(path)?;
    let commit = repository
        .rev_parse_single(commit_id)?
        .object()?
        .into_commit();
    let parent_tree = match commit.parent_ids().next() {
        Some(parent) => Some(parent.object()?.into_commit().tree()?),
        None => None,
    };
    let changes = repository.diff_tree_to_tree(parent_tree.as_ref(), &commit.tree()?, None)?;
    let Some((before, after, _)) = changes
        .into_iter()
        .filter_map(commit_change_parts)
        .find(|(_, _, path)| path == file_path)
    else {
        anyhow::bail!("file not found in commit");
    };
    let before = read_optional_blob(&repository, before)?;
    let after = read_optional_blob(&repository, after)?;
    Ok(bounded_diff_lines(
        String::from_utf8_lossy(&render_unified_diff(&before, &after)?)
            .lines()
            .map(str::to_string)
            .collect(),
        limit,
    ))
}

fn commit_file(change: ChangeDetached) -> Option<CommitFile> {
    let kind = match &change {
        ChangeDetached::Addition { .. } => ChangeKind::Added,
        ChangeDetached::Deletion { .. } => ChangeKind::Deleted,
        ChangeDetached::Modification { .. } => ChangeKind::Modified,
        ChangeDetached::Rewrite { .. } => ChangeKind::Renamed,
    };
    let previous_path = match &change {
        ChangeDetached::Rewrite {
            source_location, ..
        } => Some(source_location.to_str_lossy().into_owned()),
        _ => None,
    };
    let (_, _, path) = commit_change_parts(change)?;
    Some(CommitFile {
        kind,
        path,
        previous_path,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum MutationError {
    #[error("not a Git repository")]
    NotRepository,
    #[error("{0}")]
    Failed(String),
}

/// Commit exactly the staged index contents with the configured identity.
pub fn commit(path: &Path, message: &str) -> Result<gix::ObjectId, MutationError> {
    let repository = open_repository(path).map_err(|_| MutationError::NotRepository)?;
    commit_inner(repository, message).map_err(|error| MutationError::Failed(error.to_string()))
}

/// Stage one regular file or deletion with a locked Git index write.
pub fn stage(path: &Path) -> Result<(), MutationError> {
    let repository = open_repository(path).map_err(|_| MutationError::NotRepository)?;
    stage_inner(repository, path).map_err(|error| MutationError::Failed(error.to_string()))
}

/// Restore one index entry from HEAD while leaving the working tree untouched.
pub fn unstage(path: &Path) -> Result<(), MutationError> {
    let repository = open_repository(path).map_err(|_| MutationError::NotRepository)?;
    unstage_inner(repository, path).map_err(|error| MutationError::Failed(error.to_string()))
}

fn commit_inner(repository: Repository, message: &str) -> anyhow::Result<ObjectId> {
    if message.trim().is_empty() {
        anyhow::bail!("commit message must not be blank");
    }

    let _index_lock = gix::lock::Marker::acquire_to_hold_resource(
        repository.index_path(),
        gix::lock::acquire::Fail::Immediately,
        None,
    )?;
    let index = open_index(&repository)?;
    if index
        .entries()
        .iter()
        .any(|entry| entry.stage() != Stage::Unconflicted)
    {
        anyhow::bail!("conflicted index");
    }
    if matches!(repository.state(), Some(gix::state::InProgress::Merge)) {
        anyhow::bail!("merge in progress");
    }

    repository
        .author()
        .context("author identity is not configured")??;
    repository
        .committer()
        .context("committer identity is not configured")??;

    let parent = repository
        .head()?
        .try_into_peeled_id()?
        .map(|id| id.detach());
    let parent_tree = match parent {
        Some(parent) => Some(repository.find_commit(parent)?.tree_id()?.detach()),
        None => None,
    };

    let mut editor = repository.empty_tree().edit()?;
    for entry in index.entries() {
        if entry.flags.contains(Flags::INTENT_TO_ADD) {
            continue;
        }
        let mode = entry
            .mode
            .to_tree_entry_mode()
            .context("invalid index entry mode")?;
        editor.upsert(entry.path(&index), mode.kind(), entry.id)?;
    }
    let tree = editor.write()?.detach();
    let previous_tree =
        parent_tree.unwrap_or_else(|| ObjectId::empty_tree(repository.object_hash()));
    if tree == previous_tree {
        anyhow::bail!("no staged changes");
    }

    Ok(repository.commit("HEAD", message, tree, parent)?.detach())
}

fn stage_inner(repository: Repository, path: &Path) -> anyhow::Result<()> {
    let relative = relative_path(&repository, path)?;
    let mut index = open_index(&repository)?;
    ensure_unconflicted(&index, relative.as_slice().as_bstr())?;

    match path.exists() {
        true => stage_file(repository, path, &mut index, relative.as_slice().as_bstr())?,
        false => {
            index.remove_entries(|_, entry_path, _| entry_path == relative.as_slice().as_bstr());
        }
    }
    write_index(&mut index)
}

fn unstage_inner(repository: Repository, path: &Path) -> anyhow::Result<()> {
    let relative = relative_path(&repository, path)?;
    let mut index = open_index(&repository)?;
    ensure_unconflicted(&index, relative.as_slice().as_bstr())?;
    let head_entry = head_entry(
        &repository,
        &gix::path::from_bstr(relative.as_slice().as_bstr()),
    );

    match head_entry {
        Some((id, mode)) => upsert_index_entry(
            &mut index,
            relative.as_slice().as_bstr(),
            id,
            Mode::from(mode),
            Stat::default(),
        ),
        None => {
            index.remove_entries(|_, entry_path, _| entry_path == relative.as_slice().as_bstr());
        }
    }
    write_index(&mut index)
}

fn stage_file(
    repository: Repository,
    path: &Path,
    index: &mut IndexFile,
    relative: &BStr,
) -> anyhow::Result<()> {
    let mut pipeline = repository.filter_pipeline(None)?;
    let Some((id, kind, _)) = pipeline.0.worktree_file_to_object(relative, index)? else {
        anyhow::bail!("unsupported file type");
    };
    let mode = match kind {
        EntryKind::Blob => Mode::FILE,
        EntryKind::BlobExecutable => Mode::FILE_EXECUTABLE,
        _ => anyhow::bail!("only regular files are supported"),
    };
    let metadata = IndexMetadata::from_path_no_follow(path)?;
    let stat = Stat::from_fs(&metadata)?;
    upsert_index_entry(index, relative, id, mode, stat);
    Ok(())
}

fn upsert_index_entry(
    index: &mut IndexFile,
    relative: &BStr,
    id: ObjectId,
    mode: Mode,
    stat: Stat,
) {
    match index.entry_mut_by_path_and_stage(relative, Stage::Unconflicted) {
        Some(entry) => {
            entry.id = id;
            entry.mode = mode;
            entry.stat = stat;
        }
        None => {
            index.dangerously_push_entry(stat, id, Flags::empty(), mode, relative);
            index.sort_entries();
        }
    }
}

fn open_index(repository: &Repository) -> anyhow::Result<IndexFile> {
    Ok(IndexFile::at_or_default(
        repository.index_path(),
        repository.object_hash(),
        false,
        Default::default(),
    )?)
}

fn relative_path(repository: &Repository, path: &Path) -> anyhow::Result<BString> {
    let workdir = repository.workdir().context("working tree not found")?;
    let relative = path.strip_prefix(workdir)?;
    Ok(gix::path::try_into_bstr(relative)?.into_owned())
}

fn ensure_unconflicted(index: &IndexState, path: &BStr) -> anyhow::Result<()> {
    if index.entry_range(path).is_some_and(|range| {
        range
            .into_iter()
            .any(|idx| index.entry(idx).stage() != Stage::Unconflicted)
    }) {
        anyhow::bail!("conflicted entries are not supported");
    }
    Ok(())
}

fn write_index(index: &mut IndexFile) -> anyhow::Result<()> {
    index.remove_tree();
    index.write(Default::default())?;
    Ok(())
}

fn head_entry(repository: &Repository, relative: &Path) -> Option<(ObjectId, EntryMode)> {
    repository
        .head_commit()
        .ok()
        .and_then(|commit| commit.tree().ok())
        .and_then(|tree| tree.lookup_entry_by_path(relative).ok().flatten())
        .map(|entry| (entry.id().detach(), entry.mode()))
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("not a Git repository")]
    NotRepository,
    #[error("failed to read Git history: {0}")]
    Read(#[source] anyhow::Error),
}

/// Read one bounded newest-first history page with short IDs and one-line subjects.
pub fn history(path: &Path, offset: usize, limit: usize) -> Result<HistoryPage, HistoryError> {
    if limit == 0 {
        return Ok(HistoryPage {
            commits: Vec::new(),
            has_more: false,
        });
    }
    let repository = open_repository(path).map_err(|_| HistoryError::NotRepository)?;
    read_history(repository, offset, limit).map_err(HistoryError::Read)
}

fn open_repository(path: &Path) -> anyhow::Result<Repository> {
    if !path.is_absolute() {
        anyhow::bail!("repository path must be absolute");
    }
    let directory_probe = path.is_dir().then(|| path.join(".eggplant-vcs-history"));
    let path = directory_probe.as_deref().unwrap_or(path);
    let directory = path.parent().context("file has no parent directory")?;
    let discover_options = gix::discover::upwards::Options {
        dot_git_only: true,
        ..Default::default()
    };
    let (repository, _) = gix::discover::upwards_opts(directory, discover_options)?;
    let (git_dir, _) = repository.into_repository_and_work_tree_directories();
    let trust = gix::sec::Trust::Reduced;
    let options = gix::open::Options::default()
        .permissions(gix::open::Permissions::default_for_level(trust))
        .open_path_as_is(true)
        .with(trust);
    Ok(gix::ThreadSafeRepository::open_opts(git_dir, options)
        .context("failed to open Git repository")?
        .to_thread_local())
}

fn read_history(
    repository: Repository,
    offset: usize,
    limit: usize,
) -> anyhow::Result<HistoryPage> {
    let head = match repository.head_commit() {
        Ok(head) => head,
        Err(_) if repository.head_name().ok().flatten().is_some() => {
            // A symbolic HEAD without a commit is an unborn branch, not corrupt history.
            return Ok(HistoryPage {
                commits: Vec::new(),
                has_more: false,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let mut commits = repository
        .rev_walk([head.id])
        .all()?
        .skip(offset)
        .take(limit.saturating_add(1))
        .map(|info| {
            let info = info?;
            let commit = info.object()?;
            let subject = commit
                .message_raw_sloppy()
                .lines()
                .next()
                .map(|line| String::from_utf8_lossy(line).into_owned())
                .unwrap_or_default();
            let author = commit.author()?;
            Ok(CommitSummary {
                id: info.id.to_hex_with_len(7).to_string(),
                subject,
                author: author.name.to_str_lossy().into_owned(),
                date: author
                    .time()?
                    .format_or_unix(gix::date::time::format::SHORT),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let has_more = commits.len() > limit;
    commits.truncate(limit);
    Ok(HistoryPage { commits, has_more })
}

#[cfg(test)]
mod tests {
    use std::{path::Path, process::Command};

    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Eggplant Test")
            .env("GIT_AUTHOR_EMAIL", "eggplant@example.com")
            .env("GIT_COMMITTER_NAME", "Eggplant Test")
            .env("GIT_COMMITTER_EMAIL", "eggplant@example.com")
            .status()
            .unwrap();
        assert!(status.success(), "git command failed: {args:?}");
    }

    fn configure_identity(root: &Path) {
        git(root, &["config", "user.name", "Eggplant Test"]);
        git(root, &["config", "user.email", "eggplant@example.com"]);
    }

    fn unborn_repository() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        git(
            directory.path(),
            &["init", "--quiet", "--initial-branch=main"],
        );
        configure_identity(directory.path());
        directory
    }

    fn repository() -> tempfile::TempDir {
        let directory = unborn_repository();
        std::fs::write(directory.path().join("note.txt"), "hello\n").unwrap();
        git(directory.path(), &["add", "note.txt"]);
        git(directory.path(), &["commit", "--quiet", "-m", "initial"]);
        directory
    }

    fn head_id(root: &Path) -> ObjectId {
        open_repository(root).unwrap().head_id().unwrap().detach()
    }

    #[test]
    fn reads_branch_and_detached_head_from_a_workspace_or_file() {
        let repository = repository();
        let file = repository.path().join("note.txt");
        assert_eq!(current_head(repository.path()).as_deref(), Some("main"));
        assert_eq!(current_head(&file).as_deref(), Some("main"));

        git(repository.path(), &["checkout", "--quiet", "--detach"]);
        let detached = current_head(repository.path()).unwrap();
        assert_eq!(detached.len(), 8);
        assert!(
            detached
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }

    #[test]
    fn non_repository_unborn_head_and_relative_path_are_neutral() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(current_head(directory.path()), None);
        assert_eq!(current_head(Path::new("relative/path")), None);

        git(
            directory.path(),
            &["init", "--quiet", "--initial-branch=main"],
        );
        assert_eq!(current_head(directory.path()), None);
    }

    #[test]
    fn diff_base_reads_head_and_marks_untracked_files() {
        let repository = repository();
        assert_eq!(
            diff_base(&repository.path().join("note.txt")).unwrap(),
            Some(b"hello\n".to_vec())
        );
        assert_eq!(
            diff_base(&repository.path().join("untracked.txt")).unwrap(),
            None
        );

        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            diff_base(&directory.path().join("note.txt")),
            Err(DiffBaseError::NotRepository)
        ));
    }

    #[test]
    fn commit_diff_returns_unified_lines_for_multiple_files() {
        let repository = repository();
        std::fs::write(repository.path().join("note.txt"), "updated\n").unwrap();
        std::fs::write(repository.path().join("other.txt"), "other\n").unwrap();
        std::fs::create_dir(repository.path().join("src")).unwrap();
        std::fs::write(repository.path().join("src/new.rs"), "fn main() {}\n").unwrap();
        git(repository.path(), &["add", "."]);
        git(repository.path(), &["commit", "--quiet", "-m", "second"]);
        let commits = history(repository.path(), 0, 10).unwrap().commits;

        let diff = commit_diff(repository.path(), &commits[0].id, 100).unwrap();
        assert!(diff.lines.iter().any(|line| line == "-hello"));
        assert!(diff.lines.iter().any(|line| line == "+updated"));
        for path in ["note.txt", "other.txt", "src/new.rs"] {
            assert!(diff.lines.iter().any(|line| line == path), "missing {path}");
        }
        assert!(diff.lines.iter().all(|line| !line.contains('\u{fffd}')));

        let details = commit_details(repository.path(), &commits[0].id).unwrap();
        assert_eq!(details.full_id.len(), 40);
        assert_eq!(details.message, "second\n");
        assert_eq!(details.files.len(), 3);
        let other = commit_file_diff(repository.path(), &commits[0].id, "other.txt", 100).unwrap();
        assert!(other.lines.iter().any(|line| line == "+other"));

        let first = commit_diff(repository.path(), &commits[1].id, 100).unwrap();
        assert!(first.lines.iter().any(|line| line == "+hello"));
    }

    #[test]
    fn merge_commit_diff_uses_only_the_first_parent() {
        let repository = repository();
        git(repository.path(), &["checkout", "--quiet", "-b", "side"]);
        std::fs::write(repository.path().join("side.txt"), "side\n").unwrap();
        git(repository.path(), &["add", "side.txt"]);
        git(repository.path(), &["commit", "--quiet", "-m", "side"]);
        git(repository.path(), &["checkout", "--quiet", "main"]);
        std::fs::write(repository.path().join("main.txt"), "main\n").unwrap();
        git(repository.path(), &["add", "main.txt"]);
        git(repository.path(), &["commit", "--quiet", "-m", "main"]);
        git(
            repository.path(),
            &["merge", "--quiet", "--no-ff", "side", "-m", "merge"],
        );

        let commits = history(repository.path(), 0, 10).unwrap().commits;
        let merge = commits
            .iter()
            .find(|commit| commit.subject == "merge")
            .unwrap();
        let details = commit_details(repository.path(), &merge.id).unwrap();

        assert_eq!(details.files.len(), 1);
        assert_eq!(details.files[0].path, "side.txt");
        let diff = commit_file_diff(repository.path(), &merge.id, "side.txt", 100).unwrap();
        assert!(diff.lines.iter().any(|line| line == "+side"));
    }

    #[test]
    fn history_is_bounded_newest_first_oneline_summaries() {
        let repository = repository();
        std::fs::write(repository.path().join("note.txt"), "updated\n").unwrap();
        git(repository.path(), &["add", "note.txt"]);
        git(
            repository.path(),
            &["commit", "--quiet", "-m", "second subject\n\nbody"],
        );

        let commits = history(repository.path(), 0, 10).unwrap().commits;
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "second subject");
        assert_eq!(commits[1].subject, "initial");
        assert!(
            commits
                .iter()
                .all(|commit| commit.author == "Eggplant Test")
        );
        assert!(commits.iter().all(|commit| commit.date.len() == 10));
        assert!(commits.iter().all(|commit| commit.id.len() == 7));
        let first_page = history(repository.path(), 0, 1).unwrap();
        assert_eq!(first_page.commits.len(), 1);
        assert_eq!(first_page.commits[0].subject, "second subject");
        assert!(first_page.has_more);
        let second_page = history(repository.path(), 1, 1).unwrap();
        assert_eq!(second_page.commits.len(), 1);
        assert_eq!(second_page.commits[0].subject, "initial");
        assert!(!second_page.has_more);
        assert!(history(repository.path(), 2, 1).unwrap().commits.is_empty());
        assert!(history(repository.path(), 0, 0).unwrap().commits.is_empty());
    }

    #[test]
    fn history_lossily_decodes_non_utf8_subjects() {
        let repository = repository();
        std::fs::write(repository.path().join("note.txt"), "updated\n").unwrap();
        git(repository.path(), &["add", "note.txt"]);
        let message = repository.path().join("message.txt");
        std::fs::write(&message, b"invalid \xff subject\n").unwrap();
        git(
            repository.path(),
            &[
                "-c",
                "i18n.commitEncoding=ISO-8859-1",
                "commit",
                "--quiet",
                "--file",
                message.to_str().unwrap(),
            ],
        );

        let page = history(repository.path(), 0, 1).unwrap();

        assert_eq!(page.commits[0].subject, "invalid \u{fffd} subject");
    }

    #[tokio::test]
    async fn changes_include_modified_and_untracked_files_and_clean_repositories() {
        let repository = repository();
        assert!(changes(repository.path()).unwrap().is_empty());

        std::fs::write(repository.path().join("note.txt"), "changed\n").unwrap();
        std::fs::write(repository.path().join("new.txt"), "new\n").unwrap();
        std::fs::write(repository.path().join("space name.txt"), "space\n").unwrap();
        let changes = changes(repository.path()).unwrap();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].kind, ChangeKind::Untracked);
        assert!(!changes[0].staged);
        assert_eq!(changes[0].path, repository.path().join("new.txt"));
        assert_eq!(changes[1].kind, ChangeKind::Modified);
        assert_eq!(changes[1].path, repository.path().join("note.txt"));
        assert_eq!(changes[2].kind, ChangeKind::Untracked);
        assert_eq!(changes[2].path, repository.path().join("space name.txt"));
    }

    #[tokio::test]
    async fn changes_reject_non_repositories() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            changes(directory.path()),
            Err(ChangesError::NotRepository)
        ));
    }

    #[test]
    fn diffs_cover_unstaged_staged_and_untracked_files() {
        let repository = repository();
        let tracked = repository.path().join("note.txt");
        std::fs::write(&tracked, "changed\n").unwrap();
        let unstaged = changes(repository.path())
            .unwrap()
            .into_iter()
            .find(|change| change.path == tracked)
            .unwrap();
        let unstaged_diff = diff(&unstaged, 100).unwrap();
        assert!(unstaged_diff.lines.iter().any(|line| line == "-hello"));
        assert!(unstaged_diff.lines.iter().any(|line| line == "+changed"));

        stage(&tracked).unwrap();
        let staged = changes(repository.path())
            .unwrap()
            .into_iter()
            .find(|change| change.path == tracked && change.staged)
            .unwrap();
        let staged_diff = diff(&staged, 1).unwrap();
        assert!(staged_diff.truncated);
        assert_eq!(staged_diff.lines.len(), 1);

        let untracked_path = repository.path().join("untracked.txt");
        std::fs::write(&untracked_path, "new line\n").unwrap();
        let untracked = changes(repository.path())
            .unwrap()
            .into_iter()
            .find(|change| change.path == untracked_path)
            .unwrap();
        let untracked_diff = diff(&untracked, 100).unwrap();
        assert!(untracked_diff.lines.iter().any(|line| line == "+new line"));
    }

    #[tokio::test]
    async fn stage_and_unstage_move_files_between_sections() {
        let repository = repository();
        let new_file = repository.path().join("staged.txt");
        std::fs::write(&new_file, "staged\n").unwrap();
        stage(&new_file).unwrap();
        let staged = changes(repository.path()).unwrap();
        let entry = staged
            .iter()
            .find(|change| change.path == new_file)
            .unwrap();
        assert_eq!(entry.kind, ChangeKind::Added);
        assert!(entry.staged);

        unstage(&new_file).unwrap();
        let unstaged = changes(repository.path()).unwrap();
        let entry = unstaged
            .iter()
            .find(|change| change.path == new_file)
            .unwrap();
        assert_eq!(entry.kind, ChangeKind::Untracked);
        assert!(!entry.staged);

        let tracked = repository.path().join("note.txt");
        std::fs::write(&tracked, "changed\n").unwrap();
        stage(&tracked).unwrap();
        let staged = changes(repository.path()).unwrap();
        let entry = staged.iter().find(|change| change.path == tracked).unwrap();
        assert_eq!(entry.kind, ChangeKind::Modified);
        assert!(entry.staged);

        unstage(&tracked).unwrap();
        let unstaged = changes(repository.path()).unwrap();
        let entry = unstaged
            .iter()
            .find(|change| change.path == tracked)
            .unwrap();
        assert_eq!(entry.kind, ChangeKind::Modified);
        assert!(!entry.staged);
    }

    #[test]
    fn commit_writes_only_staged_index_contents() {
        let repository = repository();
        let tracked = repository.path().join("note.txt");
        std::fs::write(&tracked, "staged\n").unwrap();
        stage(&tracked).unwrap();
        std::fs::write(&tracked, "worktree\n").unwrap();
        std::fs::write(repository.path().join("untracked.txt"), "untracked\n").unwrap();
        let parent = head_id(repository.path());

        let id = commit(repository.path(), "staged only").unwrap();

        let repository_handle = open_repository(repository.path()).unwrap();
        assert_eq!(repository_handle.head_id().unwrap().detach(), id);
        let new_commit = repository_handle.find_commit(id).unwrap();
        assert_eq!(
            new_commit
                .parent_ids()
                .map(|id| id.detach())
                .collect::<Vec<_>>(),
            vec![parent]
        );
        assert_eq!(new_commit.message_raw_sloppy(), "staged only");
        let tree = new_commit.tree().unwrap();
        let note = tree.lookup_entry_by_path("note.txt").unwrap().unwrap();
        assert_eq!(
            repository_handle
                .find_object(note.id().detach())
                .unwrap()
                .data,
            b"staged\n"
        );
        assert!(
            tree.lookup_entry_by_path("untracked.txt")
                .unwrap()
                .is_none()
        );
        assert_eq!(std::fs::read_to_string(tracked).unwrap(), "worktree\n");
    }

    #[test]
    fn commit_rejects_blank_messages_without_moving_head() {
        let repository = repository();
        let tracked = repository.path().join("note.txt");
        std::fs::write(&tracked, "staged\n").unwrap();
        stage(&tracked).unwrap();
        let parent = head_id(repository.path());

        for message in ["", " \t\n"] {
            let error = commit(repository.path(), message).unwrap_err();
            assert!(
                matches!(&error, MutationError::Failed(message) if message.contains("blank")),
                "unexpected error: {error}"
            );
        }
        assert_eq!(head_id(repository.path()), parent);
    }

    #[test]
    fn commit_rejects_empty_staging_despite_worktree_changes() {
        let repository = repository();
        std::fs::write(repository.path().join("note.txt"), "unstaged\n").unwrap();
        std::fs::write(repository.path().join("untracked.txt"), "untracked\n").unwrap();
        let parent = head_id(repository.path());

        let error = commit(repository.path(), "nothing staged").unwrap_err();

        assert!(
            matches!(&error, MutationError::Failed(message) if message.contains("no staged changes")),
            "unexpected error: {error}"
        );
        assert_eq!(head_id(repository.path()), parent);
        assert_eq!(
            std::fs::read_to_string(repository.path().join("note.txt")).unwrap(),
            "unstaged\n"
        );
    }

    #[test]
    fn commit_rejects_missing_author_or_committer_identity() {
        let repository = repository();
        let tracked = repository.path().join("note.txt");
        std::fs::write(&tracked, "staged\n").unwrap();
        stage(&tracked).unwrap();
        let parent = head_id(repository.path());
        git(repository.path(), &["config", "--unset", "user.name"]);
        git(repository.path(), &["config", "--unset", "user.email"]);

        let isolated = gix::ThreadSafeRepository::open_opts(
            repository.path().join(".git"),
            gix::open::Options::isolated(),
        )
        .unwrap()
        .to_thread_local();
        let error = commit_inner(isolated, "missing author").unwrap_err();
        assert!(error.to_string().contains("author identity"));
        assert_eq!(head_id(repository.path()), parent);

        git(
            repository.path(),
            &["config", "author.name", "Eggplant Test"],
        );
        git(
            repository.path(),
            &["config", "author.email", "eggplant@example.com"],
        );
        let isolated = gix::ThreadSafeRepository::open_opts(
            repository.path().join(".git"),
            gix::open::Options::isolated(),
        )
        .unwrap()
        .to_thread_local();
        let error = commit_inner(isolated, "missing committer").unwrap_err();
        assert!(error.to_string().contains("committer identity"));
        assert_eq!(head_id(repository.path()), parent);
    }

    #[test]
    fn commit_creates_an_unborn_root_commit() {
        let repository = unborn_repository();
        std::fs::write(repository.path().join("root.txt"), "root\n").unwrap();
        git(repository.path(), &["add", "root.txt"]);

        let id = commit(repository.path(), "initial root").unwrap();

        let repository_handle = open_repository(repository.path()).unwrap();
        assert_eq!(repository_handle.head_id().unwrap().detach(), id);
        let root = repository_handle.find_commit(id).unwrap();
        assert_eq!(root.parent_ids().count(), 0);
        assert_eq!(root.message_raw_sloppy(), "initial root");
        let tree = root.tree().unwrap();
        let entry = tree.lookup_entry_by_path("root.txt").unwrap().unwrap();
        assert_eq!(
            repository_handle
                .find_object(entry.id().detach())
                .unwrap()
                .data,
            b"root\n"
        );
    }

    #[test]
    fn commit_rejects_a_preheld_index_lock_without_moving_head() {
        let repository = repository();
        let tracked = repository.path().join("note.txt");
        std::fs::write(&tracked, "staged\n").unwrap();
        stage(&tracked).unwrap();
        let parent = head_id(repository.path());
        let index_path = open_repository(repository.path()).unwrap().index_path();
        let _lock = gix::lock::Marker::acquire_to_hold_resource(
            index_path,
            gix::lock::acquire::Fail::Immediately,
            None,
        )
        .unwrap();

        let error = commit(repository.path(), "locked").unwrap_err();

        assert!(matches!(error, MutationError::Failed(_)));
        assert_eq!(head_id(repository.path()), parent);
    }

    #[test]
    fn history_distinguishes_non_repositories_and_unborn_branches() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            history(directory.path(), 0, 10),
            Err(HistoryError::NotRepository)
        ));

        git(
            directory.path(),
            &["init", "--quiet", "--initial-branch=main"],
        );
        assert!(history(directory.path(), 0, 10).unwrap().commits.is_empty());
    }
}
