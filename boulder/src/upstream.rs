// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! This module contains the logic to fetch upstreams from the network
//! and store them in the storage directory, as well as to share them
//! with the build containers.
//!
//! This module abstracts over different types of upstreams
//! and provides a unified interface to fetch, store, and share them.
//! Child modules implement the logic for each type of supported
//! upstream, so please consult them should you need specific logic
//! for a given upstream type.
//! Each child module is organized with structs/enums that provide
//! a high-level and opinionated interface to fetch, store and share
//! the upstreams it targets.
//! It also provides free-standing functions that implement the low-level
//! and unopinionated logic.

use std::{
    io,
    path::{Path, PathBuf},
};

use crate::recipe::Recipe;
use container::Container;
use stone_recipe::upstream::{self, SourceUri};
use thiserror::Error;
use tui::{ProgressBar, Styled};

use crate::upstream::{
    git::{Git, StoredGit},
    plain::{Plain, StoredPlain},
};

pub(crate) mod git;
pub(crate) mod plain;

/// An upstream is a backend where
/// to get source code from.
#[derive(Debug, Clone)]
pub enum Upstream {
    /// An archive containing source code, typically
    /// a tarball. In order to be usable, it must compatible with
    /// [bsdtar](https://man.freebsd.org/cgi/man.cgi?query=bsdtar&sektion=1&format=html).
    Plain(Plain),
    /// The source code is from a Git repository.
    Git(Git),
}

impl Upstream {
    /// Constructs an [Upstream] based on the information provided
    /// in the `upstream` section of a Stone recipe.
    pub fn from_recipe_upstream(upstream: upstream::Upstream, original_index: usize) -> Result<Self, Error> {
        match upstream.props {
            upstream::Props::Plain {
                hash,
                rename,
                strip_dirs,
                unpack,
                unpack_dir,
            } => Ok(Self::Plain(Plain {
                url: upstream.url,
                hash: hash.parse().map_err(plain::Error::from)?,
                rename,
                extract: unpack.then(|| plain::ExtractParams {
                    dir: unpack_dir,
                    strip_dirs: strip_dirs.unwrap_or(plain::ExtractParams::default().strip_dirs),
                }),
            })),
            upstream::Props::Git { git_ref, clone_dir } => Ok(Self::Git(Git {
                url: upstream.url,
                commit: git_ref,
                original_index,
                share_dir: clone_dir,
            })),
        }
    }

    /// Constructs a new [stone_recipe::upstream::SourceUri]
    /// where this upstream can be fetched from.
    pub fn uri(&self) -> SourceUri {
        match self {
            Upstream::Plain(plain) => SourceUri {
                url: plain.url.clone(),
                kind: upstream::Kind::Archive,
            },
            Upstream::Git(git) => SourceUri {
                url: git.url.clone(),
                kind: upstream::Kind::Git,
            },
        }
    }

    /// Returns the hash of this upstream, as a generic string.
    /// The meaning of the hash depends on the upstream variant.
    pub fn hash(&self) -> &str {
        match self {
            Upstream::Plain(plain) => &plain.hash,
            Upstream::Git(git) => &git.commit,
        }
    }

    /// Returns the path where this upstream would be stored
    /// in the storage directory.
    /// The path may be a file or a directory, depending on
    /// the upstream variant.
    /// The path may not exist yet, but it is guaranteed to
    /// be unique inside the storage directory.
    pub fn stored_path(&self, storage_dir: &Path) -> PathBuf {
        match self {
            Upstream::Plain(plain) => plain.stored_path(storage_dir),
            Upstream::Git(git) => git.stored_path(storage_dir),
        }
    }

    /// Returns the name of the upstream. This is an informal
    /// name used for logging or when a path to be created
    /// doesn't need to be unique.
    pub fn name(&self) -> &str {
        match self {
            Upstream::Plain(plain) => plain.name(),
            Upstream::Git(git) => git.name(),
        }
    }

    /// Stores the upstream into the storage directory.
    /// The final path contained in the storage directory, and the write logic,
    /// depend on the upstream variant. The final path where the upstream is stored
    /// is unique inside the storage directory.
    pub async fn store(&self, storage_dir: &Path, pb: &ProgressBar) -> Result<Stored, Error> {
        Ok(match self {
            Upstream::Plain(plain) => Stored::Plain(plain.store(storage_dir, pb).await?),
            Upstream::Git(git) => Stored::Git(git.store(storage_dir, pb).await?),
        })
    }

    /// Unconditionally removes this Upstream's resources within the storage directory.
    /// If the resources do not exist, this function returns successfully
    /// (it is idempotent).
    ///
    /// Careful: this function does not validate the content!
    /// It will be removed even if it does not belong to this Upstream.
    pub fn remove(&self, storage_dir: &Path) -> Result<(), Error> {
        match self {
            Upstream::Plain(plain) => plain.remove(storage_dir).map_err(Error::from),
            Upstream::Git(git) => git.remove(storage_dir).map_err(Error::from),
        }
    }
}

/// Information available after [Upstream] is stored on disk.
pub(crate) enum Stored {
    Plain(StoredPlain),
    Git(StoredGit),
}

impl Stored {
    /// Name of the upstream, as returned by [Upstream::name].
    pub fn name(&self) -> &str {
        match self {
            Stored::Plain(plain) => &plain.name,
            Stored::Git(git) => &git.name,
        }
    }

    /// Whether the upstream did not need to be written at the moment
    /// of being stored, because the constant was already there and valid.
    pub fn was_cached(&self) -> bool {
        match self {
            Stored::Plain(plain) => plain.was_cached,
            Stored::Git(git) => git.was_cached,
        }
    }

    /// Shares the upstream in preparation of a build.
    ///
    /// This function tries to be as efficient as possible in terms
    /// of actual bytes written/copied, by linking files from the storage directory.
    pub async fn share(&self, cont: &mut Container, dest_dir: &Path) -> Result<(), Error> {
        match self {
            Stored::Plain(plain) => plain.share(cont, dest_dir).await?,
            Stored::Git(git) => git.share(cont, dest_dir).await?,
        }
        Ok(())
    }
}

/// Returns a list of upstream from a Stone recipe.
pub fn parse_recipe(recipe: &Recipe) -> Result<Vec<Upstream>, Error> {
    recipe
        .parsed
        .upstreams
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, upstream)| Upstream::from_recipe_upstream(upstream, index))
        .collect()
}

/// Helper that removes a list of [Upstream]s from the storage directory.
pub fn remove(storage_dir: &Path, upstreams: &[Upstream]) -> Result<(), Error> {
    for upstream in upstreams {
        upstream.remove(storage_dir)?;
    }
    Ok(())
}

/// Possible errors returned by functions in this module.
#[derive(Debug, Error)]
pub enum Error {
    #[error("plain")]
    /// An error occurred while dealing with an archive-based [Upstream].
    Plain(#[from] plain::Error),
    /// An error occurred while dealing with a Git-based [Upstream].
    #[error("git")]
    Git(#[from] git::Error),
    #[error("io")]
    // A generic I/O error occurred.
    Io(#[from] io::Error),
}

/// Process git upstreams after cloning and return updated YAML if refs differ from resolved hashes.
pub(crate) fn update_git_upstream_refs(recipe_source: &str, stored_upstreams: &[Stored]) -> Option<String> {
    let mut yaml_updater = yaml::Updater::new();
    let mut refs_updated = false;

    for stored in stored_upstreams.iter() {
        if let Stored::Git(git) = stored
            && git.resolved_hash != git.original_ref
        {
            update_git_upstream_ref_in_yaml(
                &mut yaml_updater,
                git.original_index,
                git.url.as_str(),
                &git.resolved_hash,
                &git.original_ref,
            );
            println!(
                "{} | Updated ref '{}' to commit {} for {}",
                "Warning".yellow(),
                &git.resolved_hash[..8],
                git.original_ref,
                git.url.as_str()
            );
            refs_updated = true;
        }
    }

    if refs_updated {
        Some(yaml_updater.apply(recipe_source))
    } else {
        None
    }
}

/// Replaces the non-hash refs for git upstreams with the hash for the given ref
/// and includes a comment showing the original ref.
fn update_git_upstream_ref_in_yaml(
    updater: &mut yaml::Updater,
    upstream_index: usize,
    url: &str,
    new_ref: &str,
    original_ref: &str,
) {
    let git_key = format!("git|{url}");
    let new_value_with_comment = format!("{new_ref} # {original_ref}");

    // git|url: <ref>
    updater.update_value(&new_value_with_comment, |p| {
        p / "upstreams" / upstream_index / git_key.as_str()
    });

    // git|url:
    // - ref: <ref>
    // ...
    updater.update_value(&new_value_with_comment, |p| {
        p / "upstreams" / upstream_index / git_key.as_str() / "ref"
    });
}

#[cfg(test)]
mod tests {
    use crate::upstream::StoredGit;
    use crate::upstream::plain::StoredPlain;

    use super::*;

    use url::Url;

    #[test]
    fn test_update_git_upstream_refs() {
        let recipe_source = r#"
upstreams:
  - git|https://github.com/example/repo1.git: main
  - git|https://github.com/example/repo2.git:
      ref: main
  - git|https://github.com/example/repo3.git: abcd1234567890abcdef1234567890abcdef1234
  - git|https://github.com/example/repo4.git: abc123d
  - https://example.com/file.tar.gz: some-hash
"#;

        let stored = vec![
            Stored::Git(StoredGit {
                name: "repo1.git".to_owned(),
                repo: gitwrap::null_repository(),
                was_cached: false,
                url: Url::parse("https://github.com/example/repo1.git").unwrap(),
                original_ref: "main".to_owned(),
                resolved_hash: "1111222233334444555566667777888899990000".to_owned(),
                original_index: 0,
                share_dir: None,
            }),
            Stored::Git(StoredGit {
                name: "repo2.git".to_owned(),
                repo: gitwrap::null_repository(),
                was_cached: false,
                url: Url::parse("https://github.com/example/repo2.git").unwrap(),
                original_ref: "main".to_owned(),
                resolved_hash: "aaaa1111bbbb2222cccc3333dddd4444eeee5555".to_owned(),
                original_index: 1,
                share_dir: None,
            }),
            Stored::Git(StoredGit {
                name: "repo3.git".to_owned(),
                repo: gitwrap::null_repository(),
                was_cached: false,
                url: Url::parse("https://github.com/example/repo3.git").unwrap(),
                original_ref: "abcd1234567890abcdef1234567890abcdef1234".to_owned(),
                resolved_hash: "abcd1234567890abcdef1234567890abcdef1234".to_owned(),
                original_index: 2,
                share_dir: None,
            }),
            Stored::Git(StoredGit {
                name: "repo4.git".to_owned(),
                repo: gitwrap::null_repository(),
                was_cached: false,
                url: Url::parse("https://github.com/example/repo4.git").unwrap(),
                original_ref: "abc123d".to_owned(),
                resolved_hash: "abc123d567890abcdef1234567890abcdef12345".to_owned(),
                original_index: 3,
                share_dir: None,
            }),
            Stored::Git(StoredGit {
                name: "file.tar.gz".to_owned(),
                repo: gitwrap::null_repository(),
                was_cached: false,
                // We don't care about the values below.
                url: "http://example.com".try_into().unwrap(),
                original_ref: String::new(),
                resolved_hash: String::new(),
                original_index: 0,
                share_dir: None,
            }),
        ];

        let result = update_git_upstream_refs(recipe_source, &stored);

        assert!(result.is_some());
        let updated_yaml = result.unwrap();

        // Should update short form ref to hash with comment
        assert!(updated_yaml.contains("1111222233334444555566667777888899990000 # main"));

        // Should update long form ref to hash with comment
        assert!(updated_yaml.contains("aaaa1111bbbb2222cccc3333dddd4444eeee5555 # main"));

        // Should not change hash that's already a hash
        assert!(updated_yaml.contains("abcd1234567890abcdef1234567890abcdef1234"));
        assert!(
            !updated_yaml
                .contains("abcd1234567890abcdef1234567890abcdef1234 # abcd1234567890abcdef1234567890abcdef1234")
        );

        // Should update short hash to long hash
        assert!(updated_yaml.contains("abc123d567890abcdef1234567890abcdef12345 # abc123d"));

        // Should preserve non-git upstreams unchanged
        assert!(updated_yaml.contains("https://example.com/file.tar.gz: some-hash"));
    }

    #[test]
    fn test_update_git_upstream_refs_no_updates() {
        let recipe_source = r#"
upstreams:
  - git|https://github.com/example/repo3.git: abcd1234567890abcdef1234567890abcdef1234
  - https://example.com/file.tar.gz: some-hash
"#;

        let stored = vec![
            Stored::Git(StoredGit {
                name: "repo3.git".to_owned(),
                repo: gitwrap::null_repository(),
                was_cached: false,
                url: Url::parse("https://github.com/example/repo3.git").unwrap(),
                original_ref: "abcd1234567890abcdef1234567890abcdef1234".to_owned(),
                resolved_hash: "abcd1234567890abcdef1234567890abcdef1234".to_owned(),
                original_index: 0,
                share_dir: None,
            }),
            Stored::Plain(StoredPlain {
                name: "file.tar.gz".to_owned(),
                path: "/tmp/file.tar.gz".into(),
                was_cached: false,
                extract: None,
            }),
        ];

        let result = update_git_upstream_refs(recipe_source, &stored);

        assert!(result.is_none());
    }
}
