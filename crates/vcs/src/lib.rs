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
