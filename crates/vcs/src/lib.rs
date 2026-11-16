use std::path::Path;

use anyhow::Context as _;
use gix::{bstr::ByteSlice as _, sec::trust::DefaultForLevel as _};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub id: String,
    pub subject: String,
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
    let mut changes = Vec::new();
    for item in status
        .into_iter(Vec::new())
        .map_err(|error| ChangesError::Read(error.into()))?
    {
        let item = item.map_err(|error| ChangesError::Read(error.into()))?;
        if let Some(change) = change_from_status(&workdir, item) {
            changes.push(change);
        }
    }
    changes.sort_by(|left, right| {
        right
            .staged
            .cmp(&left.staged)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(changes)
}

fn change_from_status(workdir: &Path, item: gix::status::Item) -> Option<FileChange> {
    match item {
        gix::status::Item::IndexWorktree(item) => match item {
            gix::status::index_worktree::Item::Modification {
                rela_path, status, ..
            } => {
                use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
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
            gix::status::index_worktree::Item::DirectoryContents { entry, .. }
                if entry.status == gix::dir::entry::Status::Untracked =>
            {
                Some(FileChange {
                    kind: ChangeKind::Untracked,
                    staged: false,
                    path: workdir.join(entry.rela_path.to_path().ok()?),
                    previous_path: None,
                })
            }
            gix::status::index_worktree::Item::Rewrite {
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
        },
        gix::status::Item::TreeIndex(change) => match change {
            gix::diff::index::ChangeRef::Addition { location, .. } => Some(FileChange {
                kind: ChangeKind::Added,
                staged: true,
                path: workdir.join(location.to_path().ok()?),
                previous_path: None,
            }),
            gix::diff::index::ChangeRef::Deletion { location, .. } => Some(FileChange {
                kind: ChangeKind::Deleted,
                staged: true,
                path: workdir.join(location.to_path().ok()?),
                previous_path: None,
            }),
            gix::diff::index::ChangeRef::Modification { location, .. } => Some(FileChange {
                kind: ChangeKind::Modified,
                staged: true,
                path: workdir.join(location.to_path().ok()?),
                previous_path: None,
            }),
            gix::diff::index::ChangeRef::Rewrite {
                source_location,
                location,
                ..
            } => Some(FileChange {
                kind: ChangeKind::Renamed,
                staged: true,
                path: workdir.join(location.to_path().ok()?),
                previous_path: Some(workdir.join(source_location.to_path().ok()?)),
            }),
        },
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
    let head = head_entry(
        &repository,
        &gix::path::from_bstr(relative.as_slice().as_bstr()),
    )
    .map(|(id, _)| id);
    let indexed = index_entry(&repository, relative.as_slice().as_bstr());
    let (before, after) = if change.staged {
        (head, indexed)
    } else {
        (indexed, None)
    };
    let before = read_optional_blob(&repository, before)?;
    let after = if change.staged {
        read_optional_blob(&repository, after)?
    } else if change.kind == ChangeKind::Deleted {
        Vec::new()
    } else {
        std::fs::read(&change.path)?
    };
    let input = gix::diff::blob::InternedInput::new(before.as_slice(), after.as_slice());
    let diff =
        gix::diff::blob::diff_with_slider_heuristics(gix::diff::blob::Algorithm::Histogram, &input);
    let output = gix::diff::blob::UnifiedDiff::new(
        &diff,
        &input,
        gix::diff::blob::unified_diff::ConsumeBinaryHunk::new(Vec::new(), "\n"),
        gix::diff::blob::unified_diff::ContextSize::default(),
    )
    .consume()?;
    Ok(bounded_diff_lines(
        String::from_utf8_lossy(&output)
            .lines()
            .map(str::to_string)
            .collect(),
        limit,
    ))
}

fn index_entry(
    repository: &gix::Repository,
    relative: &gix::bstr::BStr,
) -> Option<gix::hash::ObjectId> {
    repository
        .index_or_empty()
        .ok()?
        .entry_by_path(relative)
        .map(|entry| entry.id)
}

fn read_optional_blob(
    repository: &gix::Repository,
    id: Option<gix::hash::ObjectId>,
) -> anyhow::Result<Vec<u8>> {
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

#[derive(Debug, thiserror::Error)]
pub enum MutationError {
    #[error("not a Git repository")]
    NotRepository,
    #[error("{0}")]
    Failed(String),
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

fn stage_inner(repository: gix::Repository, path: &Path) -> anyhow::Result<()> {
    let relative = relative_path(&repository, path)?;
    let mut index = open_index(&repository)?;
    ensure_unconflicted(&index, relative.as_slice().as_bstr())?;

    if path.exists() {
        let mut pipeline = repository.filter_pipeline(None)?;
        let Some((id, kind, _)) = pipeline
            .0
            .worktree_file_to_object(relative.as_ref(), &index)?
        else {
            anyhow::bail!("unsupported file type");
        };
        let mode = match kind {
            gix::object::tree::EntryKind::Blob => gix::index::entry::Mode::FILE,
            gix::object::tree::EntryKind::BlobExecutable => {
                gix::index::entry::Mode::FILE_EXECUTABLE
            }
            _ => anyhow::bail!("only regular files are supported"),
        };
        let metadata = gix::index::fs::Metadata::from_path_no_follow(path)?;
        let stat = gix::index::entry::Stat::from_fs(&metadata)?;
        if let Some(entry) = index
            .entry_mut_by_path_and_stage(relative.as_ref(), gix::index::entry::Stage::Unconflicted)
        {
            entry.id = id;
            entry.mode = mode;
            entry.stat = stat;
        } else {
            index.dangerously_push_entry(
                stat,
                id,
                gix::index::entry::Flags::empty(),
                mode,
                relative.as_ref(),
            );
            index.sort_entries();
        }
    } else {
        index.remove_entries(|_, entry_path, _| entry_path == relative.as_slice().as_bstr());
    }
    write_index(&mut index)
}

fn unstage_inner(repository: gix::Repository, path: &Path) -> anyhow::Result<()> {
    let relative = relative_path(&repository, path)?;
    let mut index = open_index(&repository)?;
    ensure_unconflicted(&index, relative.as_slice().as_bstr())?;
    let head_entry = head_entry(
        &repository,
        &gix::path::from_bstr(relative.as_slice().as_bstr()),
    );

    if let Some((id, mode)) = head_entry {
        let mode = gix::index::entry::Mode::from(mode);
        if let Some(entry) = index
            .entry_mut_by_path_and_stage(relative.as_ref(), gix::index::entry::Stage::Unconflicted)
        {
            entry.id = id;
            entry.mode = mode;
            entry.stat = Default::default();
        } else {
            index.dangerously_push_entry(
                Default::default(),
                id,
                gix::index::entry::Flags::empty(),
                mode,
                relative.as_ref(),
            );
            index.sort_entries();
        }
    } else {
        index.remove_entries(|_, entry_path, _| entry_path == relative.as_slice().as_bstr());
    }
    write_index(&mut index)
}

fn open_index(repository: &gix::Repository) -> anyhow::Result<gix::index::File> {
    Ok(gix::index::File::at_or_default(
        repository.index_path(),
        repository.object_hash(),
        false,
        Default::default(),
    )?)
}

fn relative_path(repository: &gix::Repository, path: &Path) -> anyhow::Result<gix::bstr::BString> {
    let workdir = repository.workdir().context("working tree not found")?;
    let relative = path.strip_prefix(workdir)?;
    Ok(gix::path::try_into_bstr(relative)?.into_owned())
}

fn ensure_unconflicted(index: &gix::index::State, path: &gix::bstr::BStr) -> anyhow::Result<()> {
    if index.entry_range(path).is_some_and(|range| {
        range
            .into_iter()
            .any(|idx| index.entry(idx).stage() != gix::index::entry::Stage::Unconflicted)
    }) {
        anyhow::bail!("conflicted entries are not supported");
    }
    Ok(())
}

fn write_index(index: &mut gix::index::File) -> anyhow::Result<()> {
    index.remove_tree();
    index.write(Default::default())?;
    Ok(())
}

fn head_entry(
    repository: &gix::Repository,
    relative: &Path,
) -> Option<(gix::hash::ObjectId, gix::object::tree::EntryMode)> {
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

/// Read a bounded newest-first history with short IDs and one-line subjects.
pub fn history(path: &Path, limit: usize) -> Result<Vec<CommitSummary>, HistoryError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let repository = open_repository(path).map_err(|_| HistoryError::NotRepository)?;
    read_history(repository, limit).map_err(HistoryError::Read)
}

fn open_repository(path: &Path) -> anyhow::Result<gix::Repository> {
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

fn read_history(repository: gix::Repository, limit: usize) -> anyhow::Result<Vec<CommitSummary>> {
    let head = match repository.head_commit() {
        Ok(head) => head,
        Err(_) if repository.head_name().ok().flatten().is_some() => {
            // A symbolic HEAD without a commit is an unborn branch, not corrupt history.
            return Ok(Vec::new());
        }
        Err(error) => return Err(error.into()),
    };
    repository
        .rev_walk([head.id])
        .all()?
        .take(limit)
        .map(|info| {
            let info = info?;
            let commit = info.object()?;
            let subject = commit
                .message_raw_sloppy()
                .lines()
                .next()
                .map(|line| String::from_utf8_lossy(line).into_owned())
                .unwrap_or_default();
            Ok(CommitSummary {
                id: info.id.to_hex_with_len(7).to_string(),
                subject,
            })
        })
        .collect()
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

    fn repository() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        git(
            directory.path(),
            &["init", "--quiet", "--initial-branch=main"],
        );
        std::fs::write(directory.path().join("note.txt"), "hello\n").unwrap();
        git(directory.path(), &["add", "note.txt"]);
        git(directory.path(), &["commit", "--quiet", "-m", "initial"]);
        directory
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
    fn history_is_bounded_newest_first_oneline_summaries() {
        let repository = repository();
        std::fs::write(repository.path().join("note.txt"), "updated\n").unwrap();
        git(repository.path(), &["add", "note.txt"]);
        git(
            repository.path(),
            &["commit", "--quiet", "-m", "second subject\n\nbody"],
        );

        let commits = history(repository.path(), 10).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "second subject");
        assert_eq!(commits[1].subject, "initial");
        assert!(commits.iter().all(|commit| commit.id.len() == 7));
        assert_eq!(history(repository.path(), 1).unwrap().len(), 1);
        assert!(history(repository.path(), 0).unwrap().is_empty());
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
    fn history_distinguishes_non_repositories_and_unborn_branches() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            history(directory.path(), 10),
            Err(HistoryError::NotRepository)
        ));

        git(
            directory.path(),
            &["init", "--quiet", "--initial-branch=main"],
        );
        assert!(history(directory.path(), 10).unwrap().is_empty());
    }
}
