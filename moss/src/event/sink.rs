// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::sync::{Arc, RwLock};

use super::{Event, Observer, Task, task};

/// Where moss sends its [`Event`]s
#[derive(Clone)]
pub struct Sink {
    observers: Arc<RwLock<Vec<Box<dyn Observer>>>>,
}

impl Sink {
    pub fn new() -> Self {
        let observers = Default::default();
        Self { observers }
    }

    pub fn add_observer(&mut self, observer: Box<dyn Observer>) {
        self.observers.write().unwrap().push(observer);
    }

    /// Send an event to the observer
    pub fn emit(&self, event: Event) {
        for observer in &*self.observers.read().unwrap() {
            observer.event(&event);
        }
    }

    /// Start a task
    pub fn start_task<T: Task>(&self, task: T) -> task::Handle<T> {
        task::Handle::new(self.clone(), None, task)
    }
}
