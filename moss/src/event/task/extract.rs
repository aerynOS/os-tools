// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use serde::Serialize;

use super::{Event, Task, Update};

/// Extracting a set of package files, with progress counting packages
#[derive(Debug, Serialize)]
pub struct ExtractPackages {
    pub count: u64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtractPackagesDetail {
    /// The package has no layout records, so there is nothing to extract
    Skipped { path: PathBuf },
}

impl Task for ExtractPackages {
    type Phase = ();
    type Detail = ExtractPackagesDetail;
    type Outcome = ();

    fn into_event(update: Update<Self>) -> Event {
        Event::ExtractPackages(update)
    }
}

/// Extracting a single package file into `root`
#[derive(Debug, Serialize)]
pub struct ExtractPackage {
    pub path: PathBuf,
    pub root: PathBuf,
}

impl Task for ExtractPackage {
    type Phase = ();
    type Detail = ();
    type Outcome = ();

    fn into_event(update: Update<Self>) -> Event {
        Event::ExtractPackage(update)
    }
}
