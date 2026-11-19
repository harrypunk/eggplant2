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
        Ok(Self {
            relative: parse_relative(input)?,
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

fn parse_relative(input: &str) -> Result<PathBuf, String> {
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
    Ok(relative)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransferDestination {
    relative: PathBuf,
}

impl TransferDestination {
    pub(crate) fn parse(input: &str) -> Result<Self, String> {
        parse_relative(input).map(|relative| Self { relative })
    }

    pub(crate) fn relative(&self) -> &Path {
        &self.relative
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

pub(crate) fn relocate(
    root: &Path,
    source: &Path,
    expected: FileKind,
    destination: &TransferDestination,
) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let source = existing_target(&root, source)?;
    let actual = file_kind(fs::symlink_metadata(&source)?.file_type())?;
    if actual != expected {
        return Err(io::Error::other("the source changed before moving"));
    }

    let destination = prepare_destination(&root, &source, expected, destination)?;
    fs::rename(source, &destination)?;
    fs::canonicalize(destination)
}

pub(crate) fn copy(
    root: &Path,
    source: &Path,
    expected: FileKind,
    destination: &TransferDestination,
) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let source = existing_target(&root, source)?;
    let actual = file_kind(fs::symlink_metadata(&source)?.file_type())?;
    if actual != expected {
        return Err(io::Error::other("the source changed before copying"));
    }

    let destination = prepare_destination(&root, &source, expected, destination)?;
    copy_entry(&source, &destination, expected)?;
    fs::canonicalize(destination)
}

fn prepare_destination(
    root: &Path,
    source: &Path,
    kind: FileKind,
    destination: &TransferDestination,
) -> io::Result<PathBuf> {
    let raw_destination = root.join(&destination.relative);
    validate_transfer_paths(source, &raw_destination, kind)?;
    ensure_missing(&raw_destination)?;
    let parent = destination
        .relative
        .parent()
        .unwrap_or(Path::new(""))
        .iter()
        .try_fold(root.to_path_buf(), |parent, name| {
            ensure_parent_directory(parent.join(name), root)
        })?;
    let name = destination.relative.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a destination name is required",
        )
    })?;
    let destination = parent.join(name);
    validate_transfer_paths(source, &destination, kind)?;
    ensure_missing(&destination)?;
    Ok(destination)
}

fn copy_entry(source: &Path, destination: &Path, kind: FileKind) -> io::Result<()> {
    match kind {
        FileKind::File => {
            fs::copy(source, destination)?;
        }
        FileKind::Directory => {
            fs::create_dir(destination)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                copy_entry(
                    &entry.path(),
                    &destination.join(entry.file_name()),
                    file_kind(entry.file_type()?)?,
                )?;
            }
        }
        FileKind::Link => {
            std::os::unix::fs::symlink(fs::read_link(source)?, destination)?;
        }
    }
    Ok(())
}

fn existing_target(root: &Path, path: &Path) -> io::Result<PathBuf> {
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
    if !parent.starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the target is outside the workspace",
        ));
    }
    Ok(parent.join(name))
}

fn validate_transfer_paths(source: &Path, destination: &Path, kind: FileKind) -> io::Result<()> {
    if source == destination {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the source and destination are the same",
        ));
    }
    if kind == FileKind::Directory && destination.starts_with(source) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a directory cannot be copied or moved inside itself",
        ));
    }
    Ok(())
}

fn ensure_missing(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the destination already exists",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn delete(root: &Path, path: &Path, expected: FileKind) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let path = existing_target(&root, path)?;
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

    #[test]
    fn move_creates_missing_parents_and_preserves_file_content() {
        let workspace = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let source = root.join("source.txt");
        fs::write(&source, "content").unwrap();
        let destination = TransferDestination::parse("nested/new.txt").unwrap();

        let moved = relocate(&root, &source, FileKind::File, &destination).unwrap();
        assert_eq!(moved, root.join("nested/new.txt"));
        assert!(!source.exists());
        assert_eq!(fs::read_to_string(moved).unwrap(), "content");
    }

    #[test]
    fn directory_move_rejects_self_descendants_and_existing_destinations() {
        let workspace = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let source = root.join("folder");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/note.txt"), "keep").unwrap();

        let inside = TransferDestination::parse("folder/subfolder").unwrap();
        assert!(relocate(&root, &source, FileKind::Directory, &inside).is_err());
        assert!(source.join("nested/note.txt").exists());

        let existing = root.join("existing");
        fs::create_dir(&existing).unwrap();
        let destination = TransferDestination::parse("existing").unwrap();
        let error = relocate(&root, &source, FileKind::Directory, &destination).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(source.exists() && existing.exists());

        let destination = TransferDestination::parse("renamed").unwrap();
        let moved = relocate(&root, &source, FileKind::Directory, &destination).unwrap();
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(moved.join("nested/note.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn move_rejects_symlink_parent_escape_and_moves_only_links() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let source = root.join("source.txt");
        fs::write(&source, "keep").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();

        let escape = TransferDestination::parse("linked/escape.txt").unwrap();
        let error = relocate(&root, &source, FileKind::File, &escape).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(source.exists());
        assert!(!outside.path().join("escape.txt").exists());

        let destination = TransferDestination::parse("moved-link").unwrap();
        relocate(&root, &root.join("linked"), FileKind::Link, &destination).unwrap();
        assert_eq!(
            fs::read_link(root.join("moved-link")).unwrap(),
            outside.path()
        );
        assert!(fs::symlink_metadata(root.join("linked")).is_err());
    }

    #[test]
    fn file_copy_creates_parents_and_preserves_source_and_content() {
        let workspace = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let source = root.join("source.txt");
        fs::write(&source, "content").unwrap();
        let existing = root.join("existing.txt");
        fs::write(&existing, "keep").unwrap();

        let rejected = TransferDestination::parse("existing.txt").unwrap();
        assert!(copy(&root, &source, FileKind::File, &rejected).is_err());
        assert_eq!(fs::read_to_string(existing).unwrap(), "keep");

        let destination = TransferDestination::parse("nested/copied.txt").unwrap();
        let copied = copy(&root, &source, FileKind::File, &destination).unwrap();
        assert_eq!(copied, root.join("nested/copied.txt"));
        assert_eq!(fs::read_to_string(&source).unwrap(), "content");
        assert_eq!(fs::read_to_string(copied).unwrap(), "content");
    }

    #[test]
    fn directory_copy_is_recursive_and_rejects_self_descendants() {
        let workspace = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let source = root.join("folder");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/note.txt"), "keep").unwrap();

        let inside = TransferDestination::parse("folder/subfolder").unwrap();
        assert!(copy(&root, &source, FileKind::Directory, &inside).is_err());
        assert!(source.join("nested/note.txt").exists());

        let destination = TransferDestination::parse("copied").unwrap();
        let copied = copy(&root, &source, FileKind::Directory, &destination).unwrap();
        assert!(source.join("nested/note.txt").exists());
        assert_eq!(
            fs::read_to_string(copied.join("nested/note.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn copy_rejects_symlink_parent_escape_and_copies_only_links() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let source = root.join("source.txt");
        fs::write(&source, "keep").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();

        let escape = TransferDestination::parse("linked/escape.txt").unwrap();
        let error = copy(&root, &source, FileKind::File, &escape).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!outside.path().join("escape.txt").exists());

        let destination = TransferDestination::parse("copied-link").unwrap();
        copy(&root, &root.join("linked"), FileKind::Link, &destination).unwrap();
        assert_eq!(
            fs::read_link(root.join("copied-link")).unwrap(),
            outside.path()
        );
        assert!(fs::symlink_metadata(root.join("linked")).is_ok());
    }
}
