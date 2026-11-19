use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};

use ignore::WalkBuilder;

const BUILT_IN_IGNORED_NAMES: &[&str] = &[".git", "node_modules", "__pycache__"];

/// Shared ignore policy for explorer and picker filesystem traversal.
pub(crate) fn is_built_in_ignored(name: &OsStr) -> bool {
    BUILT_IN_IGNORED_NAMES
        .iter()
        .any(|ignored| name == OsStr::new(ignored))
}

/// Traverse the fixed workspace with the explorer/picker ignore policy.
pub(crate) fn workspace_walk(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(true)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .filter_entry(|entry| entry.depth() == 0 || !is_built_in_ignored(entry.file_name()));
    builder
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateTarget {
    relative: PathBuf,
    directory: bool,
}

impl CreateTarget {
    pub(crate) fn parse(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err("Enter a workspace-relative path".to_string());
        }
        let path = Path::new(input);
        if path.is_absolute() {
            return Err("Use a path relative to the workspace".to_string());
        }

        let mut relative = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => relative.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err("The path must stay within the workspace".to_string());
                }
            }
        }
        if relative.as_os_str().is_empty() {
            return Err("Enter a workspace-relative path".to_string());
        }
        Ok(Self {
            relative,
            directory: input.ends_with('/'),
        })
    }

    pub(crate) fn relative(&self) -> &Path {
        &self.relative
    }

    pub(crate) fn is_directory(&self) -> bool {
        self.directory
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    File,
    Directory,
    Link,
}

pub(crate) fn create(root: &Path, target: &CreateTarget) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let parent = target
        .relative
        .parent()
        .unwrap_or(Path::new(""))
        .iter()
        .try_fold(root.clone(), |parent, name| {
            ensure_parent_directory(parent.join(name), &root)
        })?;
    let name = target.relative.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a file or directory name is required",
        )
    })?;
    let path = parent.join(name);
    if target.directory {
        fs::create_dir(&path)?;
    } else {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
    }
    fs::canonicalize(path)
}

fn ensure_parent_directory(path: PathBuf, root: &Path) -> io::Result<PathBuf> {
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let path = fs::canonicalize(path)?;
    if !path.starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the parent directory is outside the workspace",
        ));
    }
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "the parent path is not a directory",
        ));
    }
    Ok(path)
}

pub(crate) fn delete(root: &Path, path: &Path, expected: FileKind) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a file or directory name is required",
        )
    })?;
    let parent = fs::canonicalize(path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a parent directory is required",
        )
    })?)?;
    if !parent.starts_with(&root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the target is outside the workspace",
        ));
    }
    let path = parent.join(name);
    let actual = file_kind(fs::symlink_metadata(&path)?.file_type())?;
    if actual != expected {
        return Err(io::Error::other("the target changed before deletion"));
    }
    match expected {
        FileKind::Directory => fs::remove_dir_all(&path)?,
        FileKind::File | FileKind::Link => fs::remove_file(&path)?,
    }
    Ok(path)
}

fn file_kind(kind: fs::FileType) -> io::Result<FileKind> {
    if kind.is_symlink() {
        Ok(FileKind::Link)
    } else if kind.is_dir() {
        Ok(FileKind::Directory)
    } else if kind.is_file() {
        Ok(FileKind::File)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported file type",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_targets_are_relative_and_trailing_slash_selects_directories() {
        let directory = CreateTarget::parse("folder/new/").unwrap();
        assert_eq!(directory.relative(), Path::new("folder/new"));
        assert!(directory.is_directory());

        let file = CreateTarget::parse("./note.txt").unwrap();
        assert_eq!(file.relative(), Path::new("note.txt"));
        assert!(!file.is_directory());

        for invalid in ["", ".", "/tmp/outside", "../outside", "folder/../outside"] {
            assert!(CreateTarget::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn creation_creates_missing_parent_directories() {
        let workspace = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let file = CreateTarget::parse("crates/tui/src/test1/test2").unwrap();

        let created = create(&root, &file).unwrap();
        assert_eq!(created, root.join("crates/tui/src/test1/test2"));
        assert!(created.is_file());

        let directory = CreateTarget::parse("another/nested/folder/").unwrap();
        let created = create(&root, &directory).unwrap();
        assert_eq!(created, root.join("another/nested/folder"));
        assert!(created.is_dir());
    }

    #[test]
    fn creation_does_not_overwrite_or_follow_a_parent_symlink_outside() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        fs::write(root.join("note.txt"), "keep").unwrap();
        let existing = CreateTarget::parse("note.txt").unwrap();
        assert!(create(&root, &existing).is_err());
        assert_eq!(fs::read_to_string(root.join("note.txt")).unwrap(), "keep");

        std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();
        let escape = CreateTarget::parse("linked/escape.txt").unwrap();
        let error = create(&root, &escape).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!outside.path().join("escape.txt").exists());
    }

    #[test]
    fn deletion_rejects_symlink_parent_escape_and_removes_only_links() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("important.txt");
        fs::write(&outside_file, "keep").unwrap();
        std::os::unix::fs::symlink(outside.path(), workspace.path().join("linked")).unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();

        let error = delete(&root, &root.join("linked/important.txt"), FileKind::File).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_to_string(&outside_file).unwrap(), "keep");

        delete(&root, &root.join("linked"), FileKind::Link).unwrap();
        assert!(fs::symlink_metadata(root.join("linked")).is_err());
        assert_eq!(fs::read_to_string(outside_file).unwrap(), "keep");
    }
}
