// SPDX-FileCopyrightText: 2023 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use log::warn;

use crate::{
    Provider, db,
    package::{self, Package},
    repository,
};

#[derive(Debug)]
pub struct Repository {
    repository: repository::Cached,
}

impl Repository {
    pub fn new(repository: repository::Cached) -> Self {
        Self { repository }
    }

    /// Highest numerical priority "wins"
    pub fn priority(&self) -> u64 {
        self.repository.repository.priority.into()
    }

    pub fn package(&self, id: &package::Id) -> Option<Package> {
        let index_uri = self.repository.index_uri()?;

        let result = self.repository.db.get(id);

        match result {
            Ok(meta) => Some(Package {
                id: id.clone(),
                meta: package::Meta {
                    // TODO: Is there a more type-safe way to do this vs mutation? Can
                    // a new type help here?
                    uri: meta
                        .uri
                        .and_then(|relative| index_uri.join(&relative).ok())
                        .map(|url| url.to_string()),
                    ..meta
                },
                flags: package::Flags::new().with_available(),
            }),
            Err(error) if error.row_not_found() => None,
            Err(error) => {
                warn!("failed to query repository package: {error}");
                None
            }
        }
    }

    fn query(&self, flags: package::Flags, filter: db::meta::Filter<'_>) -> Vec<Package> {
        if flags.available || flags == package::Flags::default() {
            // TODO: Error handling
            let packages = match self.repository.db.query(filter) {
                Ok(packages) => packages,
                Err(error) => {
                    warn!("failed to query repository packages: {error}");
                    return vec![];
                }
            };

            packages
                .into_iter()
                .map(|(id, meta)| Package {
                    id,
                    meta,
                    flags: package::Flags::new().with_available(),
                })
                .collect()
        } else {
            vec![]
        }
    }

    pub fn list(&self, flags: package::Flags) -> Vec<Package> {
        self.query(flags, db::meta::Filter::All)
    }

    pub fn query_keyword(&self, keyword: &str, flags: package::Flags) -> Vec<Package> {
        self.query(flags, db::meta::Filter::Keyword(keyword))
    }

    /// Query all packages that match the given provider identity
    pub fn query_provider(&self, provider: &Provider, flags: package::Flags) -> Vec<Package> {
        self.query(flags, db::meta::Filter::Provider(provider.clone()))
    }

    pub fn query_name(&self, package_name: &package::Name, flags: package::Flags) -> Vec<Package> {
        self.query(flags, db::meta::Filter::Name(package_name.clone()))
    }

    pub fn query_provider_id_only(&self, provider: &Provider, flags: package::Flags) -> Vec<package::Id> {
        if flags.available || flags == package::Flags::default() {
            // TODO: Error handling
            match self.repository.db.provider_packages(provider) {
                Ok(packages) => packages,
                Err(error) => {
                    warn!("failed to query repository packages: {error}");
                    vec![]
                }
            }
        } else {
            vec![]
        }
    }
}

impl PartialEq for Repository {
    fn eq(&self, other: &Self) -> bool {
        self.repository.id.eq(&other.repository.id)
    }
}

impl Eq for Repository {}
