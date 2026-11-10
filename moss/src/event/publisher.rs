// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use super::{Event, Subscriber, Task, task};

pub struct Publisher {
    subscribers: Vec<Box<dyn Subscriber>>,
}

impl Publisher {
    pub fn new() -> Self {
        let subscribers = Default::default();
        Self { subscribers }
    }

    pub fn subscribe(&mut self, subscriber: Box<dyn Subscriber>) {
        self.subscribers.push(subscriber);
    }

    pub fn publish(&self, event: Event) {
        for subscriber in &self.subscribers {
            subscriber.event(&event);
        }
    }

    pub fn start_task<'s, T: Task>(&'s self, task: T) -> task::Handle<'s, T> {
        task::Handle::new(self, None, task)
    }
}
