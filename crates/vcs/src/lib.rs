use std::path::Path;

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
}
