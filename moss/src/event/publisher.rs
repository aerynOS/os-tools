// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use crate::event::{Update, task::Task};

use super::{Event, Subscriber};

pub struct Publisher {
    subscribers: Vec<Box<dyn Subscriber>>,
}

impl Publisher {
    pub fn new() -> Self {
        let subscribers = Vec::new();
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

    pub fn root_task<'p, S: 'p, I: 'p, O: 'p, D: 'p>(
        &'p self,
        from_update: fn(Update<S, I, O, D>) -> Event,
        info: impl Into<Option<I>>,
    ) -> Task<'p, S, I, O, D> {
        Task::root(self, from_update, info)
    }
}
