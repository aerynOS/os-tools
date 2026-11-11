// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Events moss reports instead of printing to the shell
//!
//! Events are sent to a [`Subscriber`] provided by the caller, which decides how to
//! present them, such as progress bars, json or nothing at all. This also gives
//! users a typed API instead of text to parse.

pub(crate) use self::publisher::Publisher;
pub use self::subscriber::Subscriber;
pub use self::task::Event;
pub use self::task::Task;

mod publisher;
pub mod subscriber;
pub mod task;
