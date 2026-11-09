// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::time::Duration;

use serde::Serialize;

use super::{Event, Task, Update};

/// Writing a filesystem tree to disk, with progress counting entries
#[derive(Debug, Serialize)]
pub struct Blit {
    pub total: u64,
}

/// How many entries were written, and how long it took
#[derive(Debug, Serialize)]
pub struct BlittedTree {
    pub entries: u64,
    pub elapsed: Duration,
}

impl Task for Blit {
    type Phase = ();
    type Detail = ();
    type Outcome = BlittedTree;

    fn into_event(update: Update<Self>) -> Event {
        Event::Blit(update)
    }
}
