// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{collections::HashMap, path::Path, sync::Mutex, time::Duration};

use moss::{
    Event,
    event::{
        self,
        pkg_extract::{self, ExtractPackages, PkgExtract},
        pkg_fetch::{self, FetchPackages, PkgFetch},
        task,
    },
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

impl event::Subscriber for Human {
    fn event(&self, event: &Event) {
        match event {
            Event::PkgFetch(subevent) => self.pkg_fetch(subevent),
            Event::PkgExtract(subevent) => self.pkg_extract(subevent),
        }
    }
}

impl Human {
    fn pkg_fetch(&self, event: &PkgFetch) {
        match event {
            PkgFetch::FetchPackagesTask(update) => self.fetch_packages(update),
            PkgFetch::FetchPackages(FetchPackages::FetchPackageTask(update)) => self.fetch_package(update),
        }
    }

    fn pkg_extract(&self, event: &PkgExtract) {
        match event {
            PkgExtract::ExtractPackagesTask(update) => self.extract_packages(update),
            PkgExtract::ExtractPackages(ExtractPackages::ExtractPackageTask(update)) => self.extract_package(update),
        }
    }

    fn fetch_packages(&self, update: &event::Update<pkg_fetch::PackagesInfo>) {
        match update {
            event::Update::Started { id, info, .. } => {
                let progress_bar = self.multi_progress.add(
                    ProgressBar::new(info.count).with_style(
                        ProgressStyle::with_template("\n|{bar:20.cyan/blue}| {pos}/{len}")
                            .unwrap()
                            .progress_chars("■≡=- "),
                    ),
                );
                progress_bar.tick();

                self.insert(*id, progress_bar, String::new());
            }
            event::Update::Progress { id, current, total, .. } => self.progress(*id, *current, *total),
            event::Update::Completed { id, .. } | event::Update::Aborted { id } => {
                self.remove(*id);
            }
            event::Update::Detail { .. } => {}
        }
    }

    fn fetch_package(&self, update: &event::Update<pkg_fetch::PackageInfo, pkg_fetch::PackageOutput>) {
        match update {
            event::Update::Started { id, parent, info } => {
                let progress_bar = ProgressBar::new(0)
                    .with_message(format!("{} {}", "Downloading".blue(), info.name.as_str().bold()))
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

                self.insert(*id, progress_bar, info.name.to_string());
            }
            event::Update::Progress { id, current, total, .. } => self.progress(*id, *current, *total),
            event::Update::Completed { id, output } => {
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
                        relative_to_cwd(&output.path).display()
                    );
                });
            }
            event::Update::Aborted { id } => {
                self.remove(*id);
            }
            event::Update::Detail { .. } => {}
        }
    }

    fn extract_packages(&self, update: &event::Update<pkg_extract::PackagesInfo, (), pkg_extract::PackagesDetail>) {
        if let event::Update::Detail {
            detail: pkg_extract::PackagesDetail::Skipped { path },
            ..
        } = update
        {
            println!("{path:?}: No layout records found, skipping.");
        }
    }

    fn extract_package(&self, update: &event::Update<pkg_extract::PackageInfo>) {
        if let event::Update::Started { info, .. } = update {
            println!("Extract: {:?} -> {:?}", info.path, info.root);
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
