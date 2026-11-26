// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use serde::Serialize;

use crate::{
    event::{Never, Update},
    package,
};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PkgFetch {
    FetchPackages(Update<FetchPackage, PackagesInfo>),
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct FetchPackage(pub Update<Never, PackageInfo, PackageOutput>);

/// Fetching a set of packages, with progress counting packages
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub struct PackagesInfo {
    pub count: u64,
}

/// Fetching a single package, with progress counting bytes downloaded
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub struct PackageInfo {
    pub name: package::Name,
}

/// Where a fetched package was written to
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub struct PackageOutput {
    pub path: PathBuf,
}
