// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

// Standard Library Imports
use std::sync::atomic::{AtomicU64, Ordering};

// External Crate Imports
use derive_more::Display;
use serde::Serialize;

// Local Crate Imports
use crate::{
    Event,
    event::{Publisher, Unit, Update},
};

// Public API ==========================================================================================================

// MISSING: `Id` should _not_ be user-constructable, so `Default` has been omitted.
/// Identifies a task, unique across the whole process
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, Serialize)]
pub struct Id(u64);

// Crate-Public API ====================================================================================================

/// Dropping a `Task` without completing it reports [`Update::Aborted`], so a task
/// left early by `?` is still finished for subscribers.
#[must_use]
pub(crate) struct Task<'p, I, O, D, E> {
    id: Id,
    finished: bool,
    scope: &'p Scope<'p, E>,
    from_update: Box<dyn Fn(Update<I, O, D>) -> E + 'p>,
}

impl<'p, I, O, D, E> Task<'p, I, O, D, E> {
    pub fn child_scope<E2: 'p>(&'p self, subevent_wrapper: fn(E2) -> E) -> Scope<'p, E2> {
        let parent = Some(self.id);
        let publisher = self.scope.publisher;
        let to_event = &self.scope.to_event;
        let to_event = Box::new(move |se| to_event(subevent_wrapper(se)));

        Scope {
            parent,
            publisher,
            to_event,
        }
    }

    /// Start reporting progress towards `total`, beginning at zero
    pub fn start_progress(&'p self, total: Option<u64>, unit: Unit) -> Progress<'p, I, O, D, E> {
        Progress::new(self, total, unit)
    }

    /// Report that the task finished successfully
    pub fn complete_task(mut self, output: O) {
        let id = self.id;

        self.finished = true;
        self.publish_update(Update::Completed { id, output });
    }
}

impl<'p, I, O, D, E> Drop for Task<'p, I, O, D, E> {
    fn drop(&mut self) {
        let id = self.id;

        if !self.finished {
            self.publish_update(Update::Aborted { id });
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------

pub(crate) struct Scope<'p, E> {
    parent: Option<Id>,
    publisher: &'p Publisher,
    to_event: Box<dyn Fn(E) -> Event + 'p>,
}

impl<'p, E> Scope<'p, E> {
    pub fn root(publisher: &'p Publisher, to_event: fn(E) -> Event) -> Self
    where
        E: 'p,
    {
        let parent = None;
        let to_event = Box::new(to_event);

        Self {
            parent,
            publisher,
            to_event,
        }
    }

    pub fn start_task<I: 'p, O: 'p, D: 'p>(
        &'p self,
        from_update: fn(Update<I, O, D>) -> E,
        info: I,
    ) -> Task<'p, I, O, D, E> {
        Task::new(self, from_update, info)
    }
}

pub(crate) struct Progress<'p, I, O, D, E> {
    task: &'p Task<'p, I, O, D, E>,
    current: AtomicU64,
    total: Option<u64>,
    unit: Unit,
}

impl<'p, I, O, D, E> Progress<'p, I, O, D, E> {
    pub fn set(&self, current: u64) {
        self.current.store(current, Ordering::Relaxed);
        self.publish_update();
    }

    /// Move the position forward by `delta`
    pub fn increment(&self, delta: u64) {
        self.current.fetch_add(delta, Ordering::Relaxed);
        self.publish_update();
    }
}

// Private Implementations =============================================================================================

impl Id {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        Self(id)
    }
}

impl<'p, I, O, D, E> Task<'p, I, O, D, E> {
    fn new(scope: &'p Scope<'p, E>, from_update: fn(Update<I, O, D>) -> E, info: I) -> Self
    where
        I: 'p,
        O: 'p,
        D: 'p,
    {
        let id = Id::new();
        let finished = false;
        let from_update = Box::new(from_update);
        let task = Self {
            id,
            finished,
            scope,
            from_update,
        };

        let parent = scope.parent;
        task.publish_update(Update::Started { id, parent, info });

        task
    }

    fn publish_update(&self, update: Update<I, O, D>) {
        let to_event = &self.scope.to_event;
        let from_update = &self.from_update;
        let event = to_event(from_update(update));

        self.scope.publisher.publish(event);
    }
}

impl<'p, I, O, D, E> Progress<'p, I, O, D, E> {
    fn new(task: &'p Task<'p, I, O, D, E>, total: Option<u64>, unit: Unit) -> Self {
        let current = AtomicU64::new(0);
        let progress = Self {
            task,
            current,
            total,
            unit,
        };

        progress.publish_update();
        progress
    }

    fn publish_update(&self) {
        let id = self.task.id;
        let current = self.current.load(Ordering::Relaxed);
        let total = self.total;
        let unit = self.unit;

        self.task.publish_update(Update::Progress {
            id,
            current,
            total,
            unit,
        });
    }
}
