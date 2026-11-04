// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{collections::HashMap, path::Path, sync::Mutex, time::Duration};

use moss::{
    Event,
    event::{self, task},
};
use tui::{MultiProgress, ProgressBar, ProgressStyle, Styled};

/// Renders events as progress bars and messages
#[derive(Default)]
pub struct Human {
    multi_progress: MultiProgress,
    tasks: Mutex<HashMap<task::Id, TaskProgress>>,
}

#[derive(Clone)]
struct TaskProgress {
    progress_bar: ProgressBar,
    label: String,
}

impl event::Observer for Human {
    fn event(&self, event: &Event) {
        match event {
            Event::Task(task::Event::FetchPackages(update)) => self.fetch_packages(update),
            Event::Task(task::Event::FetchPackage(update)) => self.fetch_package(update),
        }
    }
}

impl Human {
    fn fetch_packages(&self, update: &task::Update<task::FetchPackages>) {
        match update {
            task::Update::Started { id, task, .. } => {
                let progress_bar = self.multi_progress.add(
                    ProgressBar::new(task.count).with_style(
                        ProgressStyle::with_template("\n|{bar:20.cyan/blue}| {pos}/{len}")
                            .unwrap()
                            .progress_chars("■≡=- "),
                    ),
                );
                progress_bar.tick();

                self.insert(*id, progress_bar, String::new());
            }
            task::Update::Progress { id, current, total, .. } => self.progress(*id, *current, *total),
            task::Update::Completed { id, .. } | task::Update::Aborted { id } => {
                self.remove(*id);
            }
            task::Update::Phase { .. } | task::Update::Detail { .. } => {}
        }
    }

    fn fetch_package(&self, update: &task::Update<task::FetchPackage>) {
        match update {
            task::Update::Started { id, parent, task } => {
                let progress_bar = ProgressBar::new(0)
                    .with_message(format!("{} {}", "Downloading".blue(), task.name.as_str().bold()))
                    .with_style(
                        ProgressStyle::with_template(
                            " {spinner} |{percent:>3}%| {wide_msg} {binary_bytes_per_sec:>.dim} ",
                        )
                        .unwrap()
                        .tick_chars("--=≡■≡=--"),
                    );
                let progress_bar = match parent.and_then(|parent| self.task_progress(parent)) {
                    Some(parent) => self.multi_progress.insert_before(&parent.progress_bar, progress_bar),
                    None => self.multi_progress.add(progress_bar),
                };
                progress_bar.enable_steady_tick(Duration::from_millis(150));

                self.insert(*id, progress_bar, task.name.to_string());
            }
            task::Update::Progress { id, current, total, .. } => self.progress(*id, *current, *total),
            task::Update::Completed { id, outcome } => {
                let Some(task_progress) = self.remove(*id) else {
                    return;
                };

                task_progress.progress_bar.finish();
                self.multi_progress.remove(&task_progress.progress_bar);

                self.multi_progress.suspend(|| {
                    println!(
                        "{} {} {}",
                        "Fetched".green(),
                        task_progress.label.as_str().bold(),
                        relative_to_cwd(&outcome.path).display()
                    );
                });
            }
            task::Update::Aborted { id } => {
                self.remove(*id);
            }
            task::Update::Phase { .. } | task::Update::Detail { .. } => {}
        }
    }

    fn progress(&self, id: task::Id, current: u64, total: Option<u64>) {
        if let Some(task_progress) = self.task_progress(id) {
            task_progress.progress_bar.set_length(total.unwrap_or_default());
            task_progress.progress_bar.set_position(current);
        }
    }

    fn task_progress(&self, id: task::Id) -> Option<TaskProgress> {
        self.tasks.lock().unwrap().get(&id).cloned()
    }

    fn insert(&self, id: task::Id, progress_bar: ProgressBar, label: String) {
        self.tasks
            .lock()
            .unwrap()
            .insert(id, TaskProgress { progress_bar, label });
    }

    fn remove(&self, id: task::Id) -> Option<TaskProgress> {
        self.tasks.lock().unwrap().remove(&id)
    }
}

fn relative_to_cwd(path: &Path) -> &Path {
    if let Ok(cwd) = std::env::current_dir() {
        path.strip_prefix(cwd).ok().unwrap_or(path)
    } else {
        path
    }
}
