// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! A running task is reported on through its [`Handle`]. Each update about it is
//! sent as an [`Update`], wrapped in an [`Event`].

use std::{
    fmt,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::Serialize;

use super::Publisher;

pub use self::fetch::{FetchPackage, FetchPackages, FetchedPackage};

mod fetch;

/// Identifies a task, unique across the whole process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Id(u64);

impl Id {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A unit of work moss reports on
///
/// The implementing type describes the task and is reported once when it starts.
/// Tying phases, details and the outcome to the task means only valid updates
/// can be reported for it.
pub trait Task: fmt::Debug + Serialize + Sized {
    /// What the task is doing right now, one at a time
    type Phase: fmt::Debug + Serialize;
    /// A fact found or decided while the task runs
    type Detail: fmt::Debug + Serialize;
    /// What the task came to when it completed
    type Outcome: fmt::Debug + Serialize;

    fn into_event(update: Update<Self>) -> Event;
}

/// An [`Update`] about one of moss's tasks
#[derive(Debug, Serialize)]
#[serde(tag = "task", rename_all = "snake_case")]
pub enum Event {
    FetchPackages(Update<FetchPackages>),
    FetchPackage(Update<FetchPackage>),
}

/// A single update about a task
///
/// A task always sends [`Update::Started`] first and either [`Update::Completed`]
/// or [`Update::Aborted`] last.
#[derive(Debug, Serialize)]
#[serde(tag = "update", rename_all = "snake_case")]
pub enum Update<T: Task> {
    /// The task started, nested under `parent` if it has one
    Started {
        id: Id,
        parent: Option<Id>,
        #[serde(rename = "data")]
        task: T,
    },
    /// The task moved to a new phase
    Phase { id: Id, phase: T::Phase },
    /// How far along the task is
    ///
    /// Each update carries the full position, so dropping updates is safe.
    Progress {
        id: Id,
        current: u64,
        total: Option<u64>,
        unit: Unit,
    },
    /// A fact found or decided while the task runs
    Detail { id: Id, detail: T::Detail },
    /// The task finished successfully
    Completed { id: Id, outcome: T::Outcome },
    /// The task stopped before finishing, such as from an error
    Aborted { id: Id },
}

/// What an [`Update::Progress`] counts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Items,
    Bytes,
}

/// Reports on a running task
///
/// Dropping a handle without completing it reports [`Update::Aborted`], so a task
/// left early by `?` is still finished for subscribers.
#[must_use]
pub struct Handle<'s, T: Task> {
    id: Id,
    publisher: &'s Publisher,
    finished: bool,
    _task: PhantomData<fn() -> T>,
}

impl<'s, T: Task> Handle<'s, T> {
    pub(super) fn new(publisher: &'s Publisher, parent: Option<Id>, task: T) -> Self {
        let id = Id::next();

        emit(publisher, Update::Started { id, parent, task });

        Self {
            id,
            publisher,
            finished: false,
            _task: PhantomData,
        }
    }

    /// Start a task nested under this one
    pub fn start_subtask<S: Task>(&'s self, task: S) -> Handle<'s, S> {
        Handle::new(self.publisher, Some(self.id), task)
    }

    /// Start reporting progress towards `total`, beginning at zero
    ///
    /// Call again with a new total when the task starts counting something else,
    /// such as after moving to a new phase.
    pub fn start_progress(&'s self, total: Option<u64>, unit: Unit) -> Progress<'s, T> {
        let progress = Progress {
            id: self.id,
            publisher: self.publisher,
            current: Arc::new(AtomicU64::new(0)),
            total,
            unit,
            _task: PhantomData,
        };
        progress.emit(0);
        progress
    }

    /// Report that the task finished successfully
    pub fn complete_task(mut self, outcome: T::Outcome) {
        self.finished = true;
        emit(self.publisher, Update::<T>::Completed { id: self.id, outcome });
    }
}

impl<T: Task> Drop for Handle<'_, T> {
    fn drop(&mut self) {
        if !self.finished {
            emit(self.publisher, Update::<T>::Aborted { id: self.id });
        }
    }
}

/// Reports progress of a task
///
/// Cheap to clone and safe to share between threads, so parallel work can report
/// progress without passing around the task's [`Handle`].
pub struct Progress<'s, T: Task> {
    id: Id,
    publisher: &'s Publisher,
    current: Arc<AtomicU64>,
    total: Option<u64>,
    unit: Unit,
    _task: PhantomData<fn() -> T>,
}

impl<T: Task> Progress<'_, T> {
    /// Set the current position
    pub fn set(&self, current: u64) {
        self.current.store(current, Ordering::Relaxed);
        self.emit(current);
    }

    /// Move the position forward by `delta`
    pub fn inc(&self, delta: u64) {
        let current = self.current.fetch_add(delta, Ordering::Relaxed) + delta;
        self.emit(current);
    }

    fn emit(&self, current: u64) {
        emit(
            self.publisher,
            Update::<T>::Progress {
                id: self.id,
                current,
                total: self.total,
                unit: self.unit,
            },
        );
    }
}

impl<T: Task> Clone for Progress<'_, T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            publisher: self.publisher,
            current: self.current.clone(),
            total: self.total,
            unit: self.unit,
            _task: PhantomData,
        }
    }
}

fn emit<T: Task>(publisher: &Publisher, update: Update<T>) {
    publisher.publish(super::Event::Task(T::into_event(update)));
}
