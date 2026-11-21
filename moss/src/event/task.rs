// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

// Standard Library Imports
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

// External Crate Imports
use atomic_time::AtomicOptionInstant;
use derive_more::Display;
use serde::Serialize;

// Local Crate Imports
use crate::{
    Event,
    event::{Publisher, Unit, Update, UpdateKind},
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
pub(crate) struct Task<'p, S, I, O, D> {
    id: Id,
    finished: bool,
    publisher: &'p Publisher,
    // NOTE: It would be nice to pull out the `Fn(Update<S, I, O, D>) -> E` part — since that also often appears as a
    // parameter throughout this file — but that would require the `trait_alias` feature to be stabilised:
    // https://github.com/rust-lang/rust/issues/41517
    #[expect(clippy::type_complexity)]
    from_update: Box<dyn Fn(Update<S, I, O, D>) -> Event + 'p>,
}

impl<'p, S, I, O, D> Task<'p, S, I, O, D> {
    pub fn new(
        publisher: &'p Publisher,
        from_update: impl Fn(Update<S, I, O, D>) -> Event + 'p,
        info: impl Into<Option<I>>,
    ) -> Self {
        let id = Id::new();
        let finished = false;
        let from_update = Box::new(from_update);
        let task = Self {
            id,
            finished,
            publisher,
            from_update,
        };

        let info = info.into();
        task.publish_update(UpdateKind::Started(info));

        task
    }

    pub fn start_subtask<S2, I2, O2, D2>(
        &'p self,
        from_update: impl Fn(Update<S2, I2, O2, D2>) -> S + 'p,
        info: impl Into<Option<I2>>,
    ) -> Task<'p, S2, I2, O2, D2> {
        let id = self.id;
        let publisher = self.publisher;
        let from_update = Box::new(move |st| {
            let subtask = from_update(st);
            let kind = UpdateKind::Subtask(subtask);
            let update = Update { id, kind };
            let parent_from_update = &self.from_update;

            parent_from_update(update)
        });

        Task::new(publisher, from_update, info)
    }

    /// Start reporting progress towards `total`, beginning at zero
    pub fn start_progress(&'p self, total: Option<u64>, unit: Unit) -> Progress<'p, S, I, O, D> {
        Progress::new(self, total, unit)
    }

    pub fn report_detail(&self, detail: D) {
        self.publish_update(UpdateKind::Detail(detail));
    }

    /// Report that the task finished successfully
    pub fn complete(mut self, output: impl Into<Option<O>>) {
        let output = output.into();

        self.finished = true;
        self.publish_update(UpdateKind::Completed(output));
    }
}

impl<'p, S, I, O, D> Drop for Task<'p, S, I, O, D> {
    fn drop(&mut self) {
        if !self.finished {
            self.publish_update(UpdateKind::Aborted);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------

pub(crate) struct Progress<'p, S, I, O, D> {
    task: &'p Task<'p, S, I, O, D>,
    last_update: AtomicOptionInstant,
    current: AtomicU64,
    total: Option<u64>,
    unit: Unit,
}

impl<'p, S, I, O, D> Progress<'p, S, I, O, D> {
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

impl<'p, S, I, O, D> Task<'p, S, I, O, D> {
    fn publish_update(&self, kind: UpdateKind<S, I, O, D>) {
        let id = self.id;
        let update = Update { id, kind };
        let from_update = &self.from_update;
        let event = from_update(update);

        self.publisher.publish(event);
    }
}

impl<'p, S, I, O, D> Progress<'p, S, I, O, D> {
    fn new(task: &'p Task<'p, S, I, O, D>, total: Option<u64>, unit: Unit) -> Self {
        let last_update = AtomicOptionInstant::none();
        let current = AtomicU64::new(0);
        let progress = Self {
            task,
            last_update,
            current,
            total,
            unit,
        };

        progress.publish_update();
        progress
    }

    fn publish_update(&self) {
        if self.throttled() {
            return;
        }

        let current = self.current.load(Ordering::Relaxed);
        let total = self.total;
        let unit = self.unit;

        self.task.publish_update(UpdateKind::Progress { current, total, unit });

        self.last_update.store(Some(Instant::now()), Ordering::Relaxed);
    }

    fn throttled(&self) -> bool {
        const MAX_UPDATE_FREQUENCY_HZ: u64 = 10;
        const MIN_UPDATE_PERIOD: Duration = Duration::from_millis(1000 / MAX_UPDATE_FREQUENCY_HZ);

        self.last_update
            .load(Ordering::Relaxed)
            .is_some_and(|instant| instant.elapsed() < MIN_UPDATE_PERIOD)
    }
}
