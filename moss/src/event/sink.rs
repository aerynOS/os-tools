// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use super::{Event, Observer, Task, task};

/// Where moss sends its [`Event`]s
pub struct Sink {
    observers: Vec<Box<dyn Observer>>,
}

impl Sink {
    pub fn new() -> Self {
        let observers = Default::default();
        Self { observers }
    }

    pub fn add_observer(&mut self, observer: Box<dyn Observer>) {
        self.observers.push(observer);
    }

    /// Send an event to the observer
    pub fn emit(&self, event: Event) {
        for observer in &self.observers {
            observer.event(&event);
        }
    }

    /// Start a task
    pub fn start_task<'s, T: Task>(&'s self, task: T) -> task::Handle<'s, T> {
        task::Handle::new(self, None, task)
    }
}
