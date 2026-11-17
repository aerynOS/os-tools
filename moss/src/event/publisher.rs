// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use crate::event::task::Scope;

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

    pub fn root_scope<'p, E: 'p>(&'p self, to_event: fn(E) -> Event) -> Scope<'p, E> {
        Scope::root(self, to_event)
    }
}
