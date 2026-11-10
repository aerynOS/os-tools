// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use super::Event;

/// Receives every [`Event`] moss reports
///
/// Events are delivered as they happen, on whichever thread reported them, including
/// from within async code. To avoid stalling moss, a subscriber must return quickly
/// and never block, must not call back into moss and must not panic.
///
/// Some operations, such as cloning into a container, need the process to be single
/// threaded. Any thread a subscriber starts must stop by the time the task it was
/// started for finishes, so slow work should be buffered and done once moss returns.
pub trait Subscriber: Send + Sync {
    fn event(&self, event: &Event);
}
