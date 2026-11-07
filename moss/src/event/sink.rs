// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::sync::Arc;

use super::{Event, Observer, Task, observer, task};

/// Where moss sends its [`Event`]s
///
/// A sink can be scoped to a parent task (see [`task::Handle::child_sink`]), so
/// code given a sink doesn't need to know where it sits in the task tree.
#[derive(Clone)]
pub struct Sink {
    observer: Arc<dyn Observer>,
    parent: Option<task::Id>,
}

impl Sink {
    pub fn new(observer: Arc<dyn Observer>) -> Self {
        Self { observer, parent: None }
    }

    /// Send an event to the observer
    pub fn emit(&self, event: Event) {
        self.observer.event(&event);
    }

    /// Start a task, nested under this sink's parent task if it has one
    pub fn start_task<T: Task>(&self, task: T) -> task::Handle<T> {
        task::Handle::new(self.clone(), task)
    }

    pub(super) fn with_parent(&self, parent: task::Id) -> Self {
        Self {
            parent: Some(parent),
            ..self.clone()
        }
    }

    pub(super) fn parent(&self) -> Option<task::Id> {
        self.parent
    }
}

impl Default for Sink {
    fn default() -> Self {
        Self::new(Arc::new(observer::Noop))
    }
}
