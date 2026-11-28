// SPDX-FileCopyrightText: 2025 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! The system model: The enabled repositories and explicit providers that describe an installation.
//!
//! - [`SystemModel`] is the model itself as data-structure that can be operated on.
//! - [`Document`] is a model together with its KDL encoding, edited in a way that preserves the
//!   user's formatting and comments.
//! - [`Declared`] is the model the user declares at `/etc/moss/system-model.kdl` or imports via
//!   `moss sync --import <some valid system-model.kdl>`. It is a read-only data structure.
//! - [`Recorded`] is the model a fstree was built from, stored in the fstree itself as a Document
//!   at `/usr/.system-model.kdl`, next to the fstree's `/usr/.stateID` file.
//! - [`Base`] is the model a change starts from: Recorded or Declared.
//! - [`Drift`] is the difference between two models.

use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

use fs_err as fs;
use thiserror::Error;

use crate::{Provider, repository};

pub use self::decode::Error as DecodeError;

mod decode;
mod encode;
mod update;

/// Repositories and explicit providers that describe an installation
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemModel {
    pub disable_warning: bool,
    pub repositories: repository::Map,
    pub explicit: BTreeSet<Provider>,
}

impl SystemModel {
    /// Returns the [`Drift`] of `self` from `other`: Describes what changes when moving from
    /// `other` to `self`.
    pub fn drift(&self, other: &SystemModel) -> Drift {
        let mut repo_drift = BTreeMap::new();

        for (id, self_repo) in &self.repositories {
            match other.repositories.get(id) {
                // Classify the different kinds of Drift
                None => {
                    repo_drift.insert(id.clone(), RepositoryDrift::Added);
                }
                Some(other_repo) if other_repo.enabled && !self_repo.enabled => {
                    repo_drift.insert(id.clone(), RepositoryDrift::Disabled);
                }
                Some(other_repo) if !other_repo.enabled && self_repo.enabled => {
                    repo_drift.insert(id.clone(), RepositoryDrift::Enabled);
                }
                Some(other_repo) if other_repo != self_repo => {
                    repo_drift.insert(id.clone(), RepositoryDrift::Changed);
                }
                Some(_) => {}
            }
        }
        for (other_repo_id, _) in &other.repositories {
            if !self.repositories.contains_id(other_repo_id) {
                repo_drift.insert(other_repo_id.clone(), RepositoryDrift::Removed);
            }
        }

        Drift {
            repositories: repo_drift,
            explicit_added: self.explicit.difference(&other.explicit).cloned().collect(),
            explicit_removed: other.explicit.difference(&self.explicit).cloned().collect(),
        }
    }
}

/// The difference between two [`SystemModel`]s
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Drift {
    pub repositories: BTreeMap<repository::Id, RepositoryDrift>,
    pub explicit_added: BTreeSet<Provider>,
    pub explicit_removed: BTreeSet<Provider>,
}

impl Drift {
    pub fn is_empty(&self) -> bool {
        self.repositories.is_empty() && self.explicit_added.is_empty() && self.explicit_removed.is_empty()
    }
}

/// How a repository differs between two [`SystemModel`]s
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum RepositoryDrift {
    Added,
    Removed,
    Enabled,
    Disabled,
    /// Any other change, such as its source or priority
    Changed,
}

/// A [`SystemModel`] with its KDL encoding
///
/// Edits preserve the existing formatting and comments of the encoding
#[derive(Debug, Clone)]
pub struct Document {
    model: SystemModel,
    encoded: String,
}

impl Document {
    /// Encode a new document for `model`
    pub fn new(model: SystemModel) -> Self {
        let encoded = encode::encode(&model.repositories, &model.explicit);

        Self { model, encoded }
    }

    /// Decode a document from KDL
    pub fn decode(content: &str) -> Result<Self, DecodeError> {
        Ok(Self {
            model: decode::decode(content)?,
            encoded: content.to_owned(),
        })
    }

    pub fn model(&self) -> &SystemModel {
        &self.model
    }

    pub fn into_model(self) -> SystemModel {
        self.model
    }

    pub fn encoded(&self) -> &str {
        &self.encoded
    }

    /// Set the explicit providers to `explicit`.
    ///
    /// Existing providers that aren't in `explicit` are deleted, and providers of `explicit` that
    /// aren't present yet are appended in the given order, marked as added by moss.
    pub fn with_explicit<'a>(self, explicit: impl IntoIterator<Item = &'a Provider>) -> Result<Self, UpdateError> {
        let explicit = explicit.into_iter().collect::<Vec<_>>();
        let keep = explicit.iter().copied().collect::<BTreeSet<_>>();
        let add = explicit
            .into_iter()
            .filter(|provider| !self.model.explicit.contains(provider));

        let encoded = update::set_explicit(&self.encoded, &keep, add)?;

        Ok(Self::decode(&encoded)?)
    }

    /// Update the repositories of `repos` that already exist in this document, matched by
    /// [`repository::Id`]
    pub fn with_repositories(self, repos: &repository::Map) -> Result<Self, UpdateError> {
        let encoded = update::update_repositories(&self.encoded, repos.iter())?;

        Ok(Self::decode(&encoded)?)
    }
}

/// The system model the user declares, which lives in `/etc/moss/system-model.kdl` or is
/// supplied by the user with `--import`. Moss itself never writes it.
#[derive(Debug, Clone)]
pub struct Declared {
    document: Document,
    path: PathBuf,
}

impl Declared {
    /// Load the declared model at `path`, if it exists
    pub fn load(path: &Path) -> Result<Option<Self>, LoadError> {
        Ok(load(path)?.map(|document| Self {
            document,
            path: path.to_owned(),
        }))
    }

    pub fn model(&self) -> &SystemModel {
        self.document.model()
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The system model a fstree was built from. Stored in the fstree at `/usr/.system-model.kdl`.
/// Moss writes (records) the system model of every fstree it builds as a convenience.
#[derive(Debug, Clone)]
pub struct Recorded {
    document: Document,
}

impl Recorded {
    /// Where the recorded model is stored, relative to a fstree's root
    pub const PATH: &str = "usr/.system-model.kdl";

    /// Where earlier versions of moss stored it, relative to a fstree's root
    pub const LEGACY_PATH: &str = "usr/lib/system-model.kdl";

    /// The recorded model of the fstree at `root`, if the root has one (= is not detached)
    pub fn load(root: &Path) -> Result<Option<Self>, LoadError> {
        let document = match load(&root.join(Self::PATH))? {
            Some(document) => Some(document),
            None => load(&root.join(Self::LEGACY_PATH))?,
        };

        Ok(document.map(|document| Self { document }))
    }

    pub fn model(&self) -> &SystemModel {
        self.document.model()
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Write the recorded model into the fstree at `root`
    pub fn write(&self, root: &Path) -> Result<(), io::Error> {
        let path = root.join(Self::PATH);

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        fs::write(path, self.document.encoded())
    }
}

impl From<Document> for Recorded {
    fn from(document: Document) -> Self {
        Self { document }
    }
}

/// The system model a change starts from: A fstree's recorded model, or the user's declared model.
/// Moss may drop explicit providers that can no longer be installed from a recorded model.
/// Moss will never drop explicit providers from a declared model.
#[derive(Debug, Clone)]
pub enum Base {
    Recorded(Recorded),
    Declared(Declared),
}

impl Base {
    pub fn model(&self) -> &SystemModel {
        self.document().model()
    }

    pub fn document(&self) -> &Document {
        match self {
            Base::Recorded(recorded) => recorded.document(),
            Base::Declared(declared) => declared.document(),
        }
    }
}

/// Loads a [`Document`] from `path`, if it exists
fn load(path: &Path) -> Result<Option<Document>, LoadError> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(LoadError::ReadFile)?;

    Ok(Some(Document::decode(&content)?))
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("read file")]
    ReadFile(#[source] io::Error),
    #[error("decode")]
    Decode(#[from] decode::Error),
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("decode")]
    Decode(#[from] decode::Error),
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Repository;

    fn repo(priority: u64, enabled: bool) -> Repository {
        Repository {
            description: String::new(),
            source: repository::Source::DirectIndex("https://example.com/stone.index".parse().unwrap()),
            priority: repository::Priority::new(priority),
            enabled,
        }
    }

    fn model(repos: &[(&str, Repository)], explicit: &[&str]) -> SystemModel {
        SystemModel {
            disable_warning: false,
            repositories: repos
                .iter()
                .map(|(id, repo)| (repository::Id::new(id), repo.clone()))
                .collect(),
            explicit: explicit.iter().map(|p| Provider::from_name(p).unwrap()).collect(),
        }
    }

    #[test]
    fn test_drift() {
        let recorded = model(
            &[
                ("kept", repo(1, true)),
                ("removed", repo(1, true)),
                ("disabled", repo(1, true)),
                ("enabled", repo(1, false)),
                ("changed", repo(1, true)),
            ],
            &["a", "b"],
        );
        let declared = model(
            &[
                ("kept", repo(1, true)),
                ("disabled", repo(1, false)),
                ("enabled", repo(1, true)),
                ("changed", repo(2, true)),
                ("added", repo(1, true)),
            ],
            &["b", "soname(c.so)"],
        );

        let drift = declared.drift(&recorded);

        assert_eq!(
            drift
                .repositories
                .iter()
                .map(|(id, drift)| (id.to_string(), *drift))
                .collect::<Vec<_>>(),
            vec![
                ("added".to_owned(), RepositoryDrift::Added),
                ("changed".to_owned(), RepositoryDrift::Changed),
                ("disabled".to_owned(), RepositoryDrift::Disabled),
                ("enabled".to_owned(), RepositoryDrift::Enabled),
                ("removed".to_owned(), RepositoryDrift::Removed),
            ]
        );
        assert_eq!(drift.explicit_added, model(&[], &["soname(c.so)"]).explicit);
        assert_eq!(drift.explicit_removed, model(&[], &["a"]).explicit);
        assert!(declared.drift(&declared).is_empty());
    }
}
