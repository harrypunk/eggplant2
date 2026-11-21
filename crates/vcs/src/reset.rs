use std::{collections::BTreeSet, path::Path};

use anyhow::Context as _;
use gix::{
    ObjectId, Repository,
    bstr::ByteSlice as _,
    refs::{
        FullName,
        transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog},
    },
};

use super::{MutationError, open_repository, read_history};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetTarget {
    pub label: String,
    pub revision: String,
}

/// List local main/master, remote branches, and the five newest commits.
pub fn reset_targets(path: &Path) -> Result<Vec<ResetTarget>, MutationError> {
    let repository = open_repository(path).map_err(|_| MutationError::NotRepository)?;
    load_targets(repository).map_err(|error| MutationError::Failed(error.to_string()))
}

/// Move HEAD or its current branch to a commit without changing index or worktree.
pub fn reset_soft(path: &Path, target: &str) -> Result<ObjectId, MutationError> {
    let repository = open_repository(path).map_err(|_| MutationError::NotRepository)?;
    soft_inner(repository, target).map_err(|error| MutationError::Failed(error.to_string()))
}

fn load_targets(repository: Repository) -> anyhow::Result<Vec<ResetTarget>> {
    let mut local = BTreeSet::new();
    let mut remote = BTreeSet::new();
    for reference in repository.references()?.all()? {
        let reference = reference.map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let Some((category, name)) = reference.name().category_and_short_name() else {
            continue;
        };
        let name = name.to_str_lossy().into_owned();
        match category {
            gix::reference::Category::LocalBranch if name == "main" || name == "master" => {
                local.insert(name);
            }
            gix::reference::Category::RemoteBranch if !name.ends_with("/HEAD") => {
                remote.insert(name);
            }
            _ => {}
        }
    }

    let refs = ["main", "master"]
        .into_iter()
        .filter(|name| local.contains(*name))
        .map(str::to_string)
        .chain(remote);
    let commits = read_history(repository.clone(), 0, 5)?
        .commits
        .into_iter()
        .map(|commit| ResetTarget {
            label: format!("{} {}", commit.id, commit.subject),
            revision: commit.id,
        });
    Ok(refs
        .map(|name| ResetTarget {
            label: name.clone(),
            revision: name,
        })
        .chain(commits)
        .collect())
}

fn soft_inner(repository: Repository, target: &str) -> anyhow::Result<ObjectId> {
    if repository.state().is_some() {
        anyhow::bail!("repository operation in progress");
    }
    let mut head = repository.head()?;
    if head.is_unborn() {
        anyhow::bail!("unborn HEAD is not supported");
    }
    let old_id = head.peel_to_commit()?.id;
    let target_id = repository
        .rev_parse_single(target)
        .with_context(|| format!("reset target `{target}` not found"))?
        .object()?
        .into_commit()
        .id;
    let name = match head.referent_name() {
        Some(name) => name.to_owned(),
        None => FullName::try_from("HEAD").expect("HEAD is a valid reference"),
    };
    repository.edit_reference(RefEdit {
        name,
        deref: false,
        change: Change::Update {
            expected: PreviousValue::MustExistAndMatch(gix::refs::Target::Object(old_id)),
            new: gix::refs::Target::Object(target_id),
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: format!("reset: soft to {target}").into(),
            },
        },
    })?;
    Ok(target_id)
}
