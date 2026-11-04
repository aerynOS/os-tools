// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use serde::Serialize;

use super::{Event, Task, Update};
use crate::package;

/// Fetching a set of packages, with progress counting packages
#[derive(Debug, Serialize)]
pub struct FetchPackages {
    pub count: u64,
}

impl Task for FetchPackages {
    type Phase = ();
    type Detail = ();
    type Outcome = ();

    fn into_event(update: Update<Self>) -> Event {
        Event::FetchPackages(update)
    }
}

/// Fetching a single package, with progress counting bytes downloaded
#[derive(Debug, Serialize)]
pub struct FetchPackage {
    pub name: package::Name,
}

/// Where a fetched package was written to
#[derive(Debug, Serialize)]
pub struct FetchedPackage {
    pub path: PathBuf,
}

impl Task for FetchPackage {
    type Phase = ();
    type Detail = ();
    type Outcome = FetchedPackage;

    fn into_event(update: Update<Self>) -> Event {
        Event::FetchPackage(update)
    }
}
