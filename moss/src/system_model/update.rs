// SPDX-FileCopyrightText: 2025 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeSet;

use kdl::{FormatConfig, KdlDocument, KdlNode, KdlNodeFormat};

use super::decode::{self, decode_package};
use super::encode::push_child;
use crate::system_model::encode::push_repository_fields;
use crate::{Provider, Repository, repository};

/// Keep only the explicit providers in `keep`, then append `add`
pub fn set_explicit<'a>(
    content: &str,
    keep: &BTreeSet<&Provider>,
    add: impl Iterator<Item = &'a Provider>,
) -> Result<String, decode::Error> {
    let mut document: KdlDocument = content.parse().map_err(decode::Error::ParseKdlDocument)?;

    // If we already have packages defined, remove those not kept
    // TODO: Rename this to "providers" (which is the more correct term)
    let packages = if let Some(packages) = document.get_mut("packages") {
        if let Some(children) = packages.children_mut() {
            children.nodes_mut().retain(|child| {
                if let Ok(package) = decode_package(child) {
                    keep.contains(&package)
                } else {
                    false
                }
            });
        }

        packages
    }
    // Otherwise default as empty node
    else {
        document.nodes_mut().push(KdlNode::new("packages"));
        document.get_mut("packages").expect("just pushed")
    };

    // Add requested packages
    for (idx, provider) in add.enumerate() {
        push_child(packages, provider.to_name(), |node| {
            // Add whitespace / comment
            if idx == 0 {
                node.set_format(KdlNodeFormat {
                    leading: "\n    // Added by moss\n    ".to_owned(),
                    terminator: "\n".to_owned(),
                    ..Default::default()
                });
            } else {
                node.autoformat_config(&FormatConfig::builder().indent_level(1).build());
            }
        });
    }

    Ok(document.to_string())
}

pub fn update_repositories<'a>(
    content: &str,
    repositories_to_update: impl Iterator<Item = (&'a repository::Id, &'a Repository)>,
) -> Result<String, decode::Error> {
    let mut document: KdlDocument = content.parse().map_err(decode::Error::ParseKdlDocument)?;

    if let Some(existing_repos) = document.get_mut("repositories")
        && let Some(children) = existing_repos.children_mut()
    {
        for (id, repo) in repositories_to_update {
            if let Some(node) = children.get_mut(id.as_ref()) {
                *node = KdlNode::new(node.name().clone());
                push_repository_fields(node, repo);
                node.autoformat_config(&FormatConfig::builder().indent_level(1).build());
            }
        }
    }

    Ok(document.to_string())
}

#[cfg(test)]
mod test {
    use crate::system_model::Document;

    use super::*;

    const CONTENT: &str = r#"repositories

// My comment
packages {
    // my comment
    a
     b
    delete-me
    "soname(foo.so)"

  // Weird trailing comment / whitespace before closing delim
  }

// Trailing comment
"#;

    #[test]
    fn test_update_remove_all() {
        const EXPECTED: &str = "repositories

// My comment
packages {
  // Weird trailing comment / whitespace before closing delim
  }

// Trailing comment
";

        let updated = Document::decode(CONTENT).unwrap().with_explicit([]).unwrap();

        assert_eq!(updated.encoded(), EXPECTED);
    }

    #[test]
    fn test_update_add() {
        const EXPECTED: &str = r#"repositories

// My comment
packages {
    // my comment
    a
     b
    delete-me
    "soname(foo.so)"

    // Added by moss
    c
    "soname(asdf.so)"

  // Weird trailing comment / whitespace before closing delim
  }

// Trailing comment
"#;

        let updated = Document::decode(CONTENT)
            .unwrap()
            .with_explicit(&[
                provider("a"),
                provider("b"),
                provider("delete-me"),
                provider(r#"soname(foo.so)"#),
                // Added
                provider("c"),
                provider(r#"soname(asdf.so)"#),
            ])
            .unwrap();

        assert_eq!(updated.encoded(), EXPECTED);
    }

    #[test]
    fn test_update_full() {
        const EXPECTED: &str = r#"repositories

// My comment
packages {
    // my comment
    a
     b
    "soname(foo.so)"

    // Added by moss
    c
    "soname(asdf.so)"

  // Weird trailing comment / whitespace before closing delim
  }

// Trailing comment
"#;

        let updated = Document::decode(CONTENT)
            .unwrap()
            .with_explicit(&[
                // Original
                provider("a"),
                provider("b"),
                provider(r#"soname(foo.so)"#),
                // Added
                provider("c"),
                provider(r#"soname(asdf.so)"#),
            ])
            .unwrap();

        assert_eq!(updated.encoded(), EXPECTED);
    }

    fn provider(name: &'static str) -> Provider {
        Provider::from_name(name).unwrap()
    }
}
