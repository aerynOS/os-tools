// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use serde::Serialize;

use super::Update;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PkgExtract {
    ExtractPackagesTask(Update<PackagesInfo, (), PackagesDetail>),
    ExtractPackages(ExtractPackages),
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractPackages {
    ExtractPackageTask(Update<PackageInfo>),
}

/// Extracting a set of package files, with progress counting packages
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub struct PackagesInfo {
    pub count: u64,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackagesDetail {
    /// The package has no layout records, so there is nothing to extract
    Skipped { path: PathBuf },
}

/// Extracting a single package file into `root`
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub struct PackageInfo {
    pub path: PathBuf,
    pub root: PathBuf,
}
