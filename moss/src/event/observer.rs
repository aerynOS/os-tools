// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::sync::Arc;

use super::Event;

/// Receives every [`Event`] moss reports
///
/// Events are delivered as they happen, on whichever thread reported them, including
/// from within async code. To avoid stalling moss an observer must return quickly
/// and never block, must not call back into moss and must not panic.
///
/// Some operations, such as cloning into a container, need the process to be single
/// threaded. Any thread an observer starts must stop by the time the task it was
/// started for finishes, so slow work should be buffered and done once moss returns.
pub trait Observer: Send + Sync {
    fn event(&self, event: &Event);
}

/// An [`Observer`] that ignores all events
pub struct Noop;

impl Observer for Noop {
    fn event(&self, _event: &Event) {}
}

/// An [`Observer`] that forwards each event to several observers, in order
///
/// Useful for combining independent outputs, such as rendering progress while
/// also collecting timings.
pub struct Tee(Vec<Arc<dyn Observer>>);

impl Tee {
    pub fn new(observers: impl IntoIterator<Item = Arc<dyn Observer>>) -> Self {
        Self(observers.into_iter().collect())
    }
}

impl Observer for Tee {
    fn event(&self, event: &Event) {
        for observer in &self.0 {
            observer.event(event);
        }
    }
}
