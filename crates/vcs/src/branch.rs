use std::{collections::BTreeSet, io::Write as _, path::Path};

use anyhow::Context as _;
use gix::{
    ObjectId, Repository,
    bstr::{BString, ByteSlice as _},
    hash::ObjectId as GixObjectId,
    index::{
        File as IndexFile,
        entry::{Mode, Stage},
    },
    object::tree::EntryMode,
    refs::{
        FullName, Target,
        transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog},
    },
};

use super::{
    MutationError, checkout_tree_preserving, open_index, open_repository, relative_path,
    remove_old_only_paths, repository_changes,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchStartPoint {
    Head,
    Branch(String),
    Revision(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchSwitchOutcome {
    pub branch: String,
    pub commit: ObjectId,
    pub created: bool,
}

enum SwitchTarget {
    Existing(String),
    Create {
        name: String,
        start: BranchStartPoint,
    },
}

/// List local branch names in deterministic order.
pub fn local_branches(path: &Path) -> Result<Vec<String>, MutationError> {
    let repository = open_repository(path).map_err(|_| MutationError::NotRepository)?;
    repository
        .references()
        .map_err(|error| MutationError::Failed(error.to_string()))?
        .all()
        .map_err(|error| MutationError::Failed(error.to_string()))?
        .try_fold(BTreeSet::new(), |mut branches, reference| {
            let reference = reference.map_err(|error| anyhow::anyhow!(error.to_string()))?;
            if let Some((gix::reference::Category::LocalBranch, name)) =
                reference.name().category_and_short_name()
            {
                branches.insert(name.to_str_lossy().into_owned());
            }
            Ok::<_, anyhow::Error>(branches)
        })
        .map(|branches| branches.into_iter().collect())
        .map_err(|error| MutationError::Failed(error.to_string()))
}

/// Switch to an existing local branch without moving its tip.
pub fn switch_branch(path: &Path, branch: &str) -> Result<BranchSwitchOutcome, MutationError> {
    switch(path, SwitchTarget::Existing(branch.to_string()))
}

/// Create a local branch at a typed start point and switch to it.
pub fn create_branch_and_switch(
    path: &Path,
    branch: &str,
    start: BranchStartPoint,
) -> Result<BranchSwitchOutcome, MutationError> {
    switch(
        path,
        SwitchTarget::Create {
            name: branch.to_string(),
            start,
        },
    )
}

fn switch(path: &Path, target: SwitchTarget) -> Result<BranchSwitchOutcome, MutationError> {
    let repository = open_repository(path).map_err(|_| MutationError::NotRepository)?;
    switch_inner(repository, target).map_err(|error| MutationError::Failed(error.to_string()))
}

fn switch_inner(
    repository: Repository,
    target: SwitchTarget,
) -> anyhow::Result<BranchSwitchOutcome> {
    if repository.state().is_some() {
        anyhow::bail!("repository operation in progress");
    }
    let mut head = repository.head()?;
    if head.is_unborn() {
        anyhow::bail!("unborn HEAD is not supported");
    }
    let old_id = head.peel_to_commit()?.id;
    let previous_head = head.referent_name().map_or(Target::Object(old_id), |name| {
        Target::Symbolic(name.to_owned())
    });
    let (branch, target_ref, target_id, created) = match target {
        SwitchTarget::Existing(branch) => {
            let target_ref = branch_reference(&branch)?;
            let target_id = local_branch_commit(&repository, &branch)?;
            (branch, target_ref, target_id, false)
        }
        SwitchTarget::Create { name, start } => {
            let target_ref = branch_reference(&name)?;
            let target_id = match start {
                BranchStartPoint::Head => old_id,
                BranchStartPoint::Branch(branch) => local_branch_commit(&repository, &branch)?,
                BranchStartPoint::Revision(revision) => {
                    repository
                        .rev_parse_single(revision.as_str())?
                        .object()?
                        .into_commit()
                        .id
                }
            };
            (name, target_ref, target_id, true)
        }
    };
    if !created && head.referent_name() == Some(target_ref.as_ref()) {
        return Ok(BranchSwitchOutcome {
            branch,
            commit: target_id,
            created,
        });
    }

    let mut index_lock = Some(gix::lock::File::acquire_to_update_resource(
        repository.index_path(),
        gix::lock::acquire::Fail::Immediately,
        None,
    )?);
    let old_index = open_index(&repository)?;
    validate_switch_index(&old_index)?;
    let target_tree = repository.find_commit(target_id)?.tree_id()?.detach();
    let preserved_paths = preserved_paths(&repository, old_id, target_id)?;
    let workdir = repository
        .workdir()
        .context("working tree not found")?
        .to_path_buf();
    let mut new_index = repository.index_from_tree(&target_tree)?;
    validate_switch_index(&new_index)?;
    reject_ignored_collisions(&repository, old_id, &new_index, &workdir, &preserved_paths)?;
    preserve_index_entries(&old_index, &mut new_index, &preserved_paths);

    let head_edit = RefEdit {
        name: "HEAD".try_into().expect("HEAD is a valid reference"),
        deref: false,
        change: Change::Update {
            expected: PreviousValue::MustExistAndMatch(previous_head),
            new: Target::Symbolic(target_ref.clone()),
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: format!("checkout: moving to {branch}").into(),
            },
        },
    };
    let edits = if created {
        vec![
            RefEdit {
                name: target_ref,
                deref: false,
                change: Change::Update {
                    expected: PreviousValue::MustNotExist,
                    new: Target::Object(target_id),
                    log: LogChange {
                        mode: RefLog::AndReference,
                        force_create_reflog: false,
                        message: format!("branch: Created from {target_id}").into(),
                    },
                },
            },
            head_edit,
        ]
    } else {
        vec![head_edit]
    };
    let committer = repository.committer().transpose()?;
    let mut prepared = Some(repository.refs.transaction().prepare(
        edits,
        gix::lock::acquire::Fail::Immediately,
        gix::lock::acquire::Fail::Immediately,
    )?);

    let result: anyhow::Result<()> = (|| {
        remove_old_only_paths(&workdir, &old_index, &new_index)?;
        let mut checked_out =
            checkout_tree_preserving(&repository, new_index.clone(), &workdir, &preserved_paths)?;
        if !created && local_branch_commit(&repository, &branch)? != target_id {
            anyhow::bail!("target branch changed during checkout");
        }
        checked_out.remove_tree();
        checked_out.write_to(index_lock.as_mut().unwrap(), Default::default())?;
        index_lock.as_mut().unwrap().flush()?;
        index_lock.take().unwrap().commit()?;
        prepared.take().unwrap().commit(committer)?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(index_lock);
        drop(prepared);
        let rollback = restore_worktree(
            &repository,
            &workdir,
            &old_index,
            &new_index,
            &preserved_paths,
        );
        if let Err(rollback) = rollback {
            return Err(error.context(format!("rollback failed: {rollback:#}")));
        }
        return Err(error);
    }

    Ok(BranchSwitchOutcome {
        branch,
        commit: target_id,
        created,
    })
}

fn branch_reference(branch: &str) -> anyhow::Result<FullName> {
    let name = format!("refs/heads/{branch}");
    gix::validate::reference::branch_name(name.as_bytes().as_bstr())?;
    Ok(FullName::try_from(name)?)
}

fn local_branch_commit(repository: &Repository, branch: &str) -> anyhow::Result<ObjectId> {
    let name = branch_reference(branch)?;
    let mut reference = repository
        .find_reference(name.as_ref())
        .with_context(|| format!("local branch `{branch}` not found"))?;
    Ok(reference.peel_to_commit()?.id)
}

fn validate_switch_index(index: &gix::index::State) -> anyhow::Result<()> {
    if index.entries().iter().any(|entry| {
        entry.stage() != Stage::Unconflicted
            || entry.mode == Mode::COMMIT
            || entry.mode == Mode::DIR
            || entry
                .flags
                .contains(gix::index::entry::Flags::SKIP_WORKTREE)
    }) {
        anyhow::bail!("conflicted, sparse, or unsupported index entries");
    }
    Ok(())
}

fn preserved_paths(
    repository: &Repository,
    old_id: ObjectId,
    target_id: ObjectId,
) -> anyhow::Result<BTreeSet<BString>> {
    let changes =
        repository_changes(repository).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut preserved = BTreeSet::new();
    for change in changes {
        if change.kind == super::ChangeKind::Conflict {
            anyhow::bail!("conflicted entries are not supported");
        }
        for path in [Some(change.path), change.previous_path]
            .into_iter()
            .flatten()
        {
            let relative = relative_path(repository, &path)?;
            if tree_entry(repository, old_id, relative.as_bstr())?
                != tree_entry(repository, target_id, relative.as_bstr())?
            {
                anyhow::bail!(
                    "local changes would be overwritten: {}",
                    relative.to_str_lossy()
                );
            }
            preserved.insert(relative);
        }
    }
    Ok(preserved)
}

fn tree_entry(
    repository: &Repository,
    commit: ObjectId,
    path: &gix::bstr::BStr,
) -> anyhow::Result<Option<(GixObjectId, EntryMode)>> {
    let tree = repository.find_commit(commit)?.tree()?;
    Ok(tree
        .lookup_entry_by_path(gix::path::from_bstr(path))?
        .map(|entry| (entry.id().detach(), entry.mode())))
}

fn reject_ignored_collisions(
    repository: &Repository,
    old_id: ObjectId,
    new_index: &gix::index::State,
    workdir: &Path,
    preserved_paths: &BTreeSet<BString>,
) -> anyhow::Result<()> {
    for entry in new_index.entries() {
        let path = entry.path(new_index).to_owned();
        if preserved_paths.contains(&path)
            || tree_entry(repository, old_id, path.as_bstr())?.is_some()
        {
            continue;
        }
        if std::fs::symlink_metadata(workdir.join(gix::path::from_bstr(path.as_bstr()))).is_ok() {
            anyhow::bail!(
                "untracked or ignored file would be overwritten: {}",
                path.to_str_lossy()
            );
        }
    }
    Ok(())
}

fn preserve_index_entries(
    old_index: &gix::index::State,
    new_index: &mut gix::index::State,
    preserved_paths: &BTreeSet<BString>,
) {
    for path in preserved_paths {
        let entries = old_index
            .entries()
            .iter()
            .filter(|entry| entry.path(old_index) == path.as_bstr())
            .cloned()
            .collect::<Vec<_>>();
        new_index.remove_entries(|_, entry_path, _| entry_path == path.as_bstr());
        for entry in entries {
            new_index.dangerously_push_entry(
                entry.stat,
                entry.id,
                entry.flags,
                entry.mode,
                path.as_bstr(),
            );
        }
    }
    new_index.sort_entries();
}

fn restore_worktree(
    repository: &Repository,
    workdir: &Path,
    old_index: &IndexFile,
    new_index: &IndexFile,
    preserved_paths: &BTreeSet<BString>,
) -> anyhow::Result<()> {
    remove_old_only_paths(workdir, new_index, old_index)?;
    let mut restored =
        checkout_tree_preserving(repository, old_index.clone(), workdir, preserved_paths)?;
    let mut lock = gix::lock::File::acquire_to_update_resource(
        repository.index_path(),
        gix::lock::acquire::Fail::Immediately,
        None,
    )?;
    restored.remove_tree();
    restored.write_to(&mut lock, Default::default())?;
    lock.flush()?;
    lock.commit()?;
    Ok(())
}
