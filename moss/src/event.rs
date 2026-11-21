// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Events moss reports instead of printing to the shell
//!
//! Events are sent to a [`Subscriber`] provided by the caller, which decides how to
//! present them, such as progress bars, json or nothing at all. This also gives
//! users a typed API instead of text to parse.

pub mod pkg_extract;
pub mod pkg_fetch;
mod publisher;
mod subscriber;
pub mod task;

// Re-Exports
pub(crate) use publisher::Publisher;
pub use subscriber::Subscriber;

// External Crate Imports
use derive_more::Display;
use serde::Serialize;

// Local Crate Imports
use pkg_extract::PkgExtract;
use pkg_fetch::PkgFetch;

// Public API ==========================================================================================================

// NOTE: By convention, `*Task` variants are leaves of the event tree that always take an `Update` — nesting is handled
// by the non-Task-suffixed variants
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    PkgFetch(Update<PkgFetch>),
    PkgExtract(Update<PkgExtract>),
}

// ---------------------------------------------------------------------------------------------------------------------

/// A single update about a task
///
/// A task always sends [`UpdateKind::Started`] first and either [`UpdateKind::Completed`]
/// or [`UpdateKind::Aborted`] last.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(tag = "update", rename_all = "snake_case")]
pub struct Update<S = Never, I = Never, O = Never, D = Never> {
    pub id: task::Id,
    pub kind: UpdateKind<S, I, O, D>,
}

// ---------------------------------------------------------------------------------------------------------------------

// TODO: When our MSRV becomes >=1.100, delete this type and replace it with `!`
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub enum Never {}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateKind<S, I, O, D> {
    /// The task started
    Started {
        info: Option<I>,
    },
    /// How far along the task is
    ///
    /// Each update carries the full position, so dropping updates is safe.
    Progress {
        current: u64,
        total: Option<u64>,
        unit: Unit,
    },
    /// A fact found or decided while the task runs
    Detail {
        detail: D,
    },
    /// The task finished successfully
    Completed {
        output: Option<O>,
    },
    /// The task stopped before finishing, such as from an error
    Aborted,
    Subtask {
        subtask: S,
    },
}

// ---------------------------------------------------------------------------------------------------------------------

/// What an [`Update::Progress`] counts
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Items,
    Bytes,
}
