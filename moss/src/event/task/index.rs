// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use serde::Serialize;

use super::{Event, Task, Update};

/// Indexing a directory of packages, with progress counting packages
#[derive(Debug, Serialize)]
pub struct IndexPackages {
    pub count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexPackagesPhase {
    /// Reading each package
    Indexing,
    /// Writing the index file
    Writing,
}

/// Where the index file was written to
#[derive(Debug, Serialize)]
pub struct IndexedPackages {
    pub path: PathBuf,
}

impl Task for IndexPackages {
    type Phase = IndexPackagesPhase;
    type Detail = ();
    type Outcome = IndexedPackages;

    fn into_event(update: Update<Self>) -> Event {
        Event::IndexPackages(update)
    }
}

/// Indexing a single package
#[derive(Debug, Serialize)]
pub struct IndexPackage {
    pub path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexPackagePhase {
    /// Hashing the package file, with progress counting bytes
    Hashing,
    /// Reading the package metadata
    Indexing,
}

impl Task for IndexPackage {
    type Phase = IndexPackagePhase;
    type Detail = ();
    type Outcome = ();

    fn into_event(update: Update<Self>) -> Event {
        Event::IndexPackage(update)
    }
}
