// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::HashMap,
    io::{self, Write},
    sync::Mutex,
    time::{Duration, Instant},
};

use moss::{
    Event,
    event::{self, task},
};

/// How often progress is written per task, since progress can update far faster than
/// is useful to consumers
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Writes each event to stdout as a line of json
#[derive(Default)]
pub struct Json {
    tasks: Mutex<HashMap<task::Id, Throttle>>,
}

struct Throttle {
    written: Instant,
    pending: Option<String>,
}

impl event::Observer for Json {
    fn event(&self, event: &Event) {
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };

        match event {
            Event::Task(task) => self.task(task, line),
        }
    }
}

impl Json {
    fn task(&self, task: &task::Event, line: String) {
        let mut tasks = self.tasks.lock().unwrap();
        let id = task.id();

        if task.is_progress() {
            if let Some(throttle) = tasks.get_mut(&id)
                && throttle.written.elapsed() < PROGRESS_INTERVAL
            {
                throttle.pending = Some(line);
                return;
            }

            tasks.insert(
                id,
                Throttle {
                    written: Instant::now(),
                    pending: None,
                },
            );
            write([line]);
        } else {
            let pending = if task.is_finished() {
                tasks.remove(&id).and_then(|throttle| throttle.pending)
            } else {
                tasks.get_mut(&id).and_then(|throttle| throttle.pending.take())
            };
            write(pending.into_iter().chain([line]));
        }
    }
}

fn write(lines: impl IntoIterator<Item = String>) {
    let mut stdout = io::stdout().lock();
    for line in lines {
        let _ = writeln!(stdout, "{line}");
    }
}
