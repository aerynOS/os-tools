// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Events moss reports instead of printing to the shell
//!
//! Events are sent to an [`Observer`] provided by the caller, which decides how to
//! present them, such as progress bars, json or nothing at all. This also gives
//! users a typed API instead of text to parse.

use serde::Serialize;

pub use self::observer::Observer;
pub(crate) use self::sink::Sink;
pub use self::task::Task;

pub mod observer;
mod sink;
pub mod task;

/// Everything moss reports
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// An update about a [`Task`]
    Task(task::Event),
}
