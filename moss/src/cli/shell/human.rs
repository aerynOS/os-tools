// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{collections::HashMap, path::Path, sync::Mutex, time::Duration};

use moss::{
    Event,
    event::{
        self, Never, Update, UpdateKind,
        pkg_extract::{self, ExtractPackage, PkgExtract},
        pkg_fetch::{self, FetchPackage, PkgFetch},
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
        // FIXME: This is looking pretty nasty — need to come up with a cleaner way to parse things
        match event {
            Event::PkgFetch(Update {
                kind: UpdateKind::Subtask(subtask),
                ..
            }) => self.pkg_fetch(subtask),
            Event::PkgExtract(Update {
                kind: UpdateKind::Subtask(subtask),
                ..
            }) => self.pkg_extract(subtask),
            // FIXME: Do something about `Update::Started`
            _ => {}
        }
    }
}

impl Human {
    fn pkg_fetch(&self, event: &PkgFetch) {
        match event {
            // FIXME: Unnest this?
            PkgFetch::FetchPackages(Update {
                id: parent,
                kind: UpdateKind::Subtask(FetchPackage(update)),
            }) => self.fetch_package(*parent, update),
            PkgFetch::FetchPackages(update) => self.fetch_packages(update),
        }
    }

    fn pkg_extract(&self, event: &PkgExtract) {
        match event {
            PkgExtract::ExtractPackages(Update {
                kind: UpdateKind::Subtask(ExtractPackage(update)),
                ..
            }) => self.extract_package(update),
            PkgExtract::ExtractPackages(update) => self.extract_packages(update),
        }
    }

    fn fetch_packages(&self, update: &Update<FetchPackage, pkg_fetch::PackagesInfo>) {
        let id = update.id;
        match &update.kind {
            UpdateKind::Started(Some(info)) => {
                let progress_bar = self.multi_progress.add(
                    ProgressBar::new(info.count).with_style(
                        ProgressStyle::with_template("\n|{bar:20.cyan/blue}| {pos}/{len}")
                            .unwrap()
                            .progress_chars("■≡=- "),
                    ),
                );
                progress_bar.tick();

                self.insert(id, progress_bar, String::new());
            }
            UpdateKind::Progress { current, total, .. } => self.progress(id, *current, *total),
            UpdateKind::Completed(_) | UpdateKind::Aborted => {
                self.remove(id);
            }
            _ => {}
        }
    }

    fn fetch_package(
        &self,
        parent: task::Id,
        update: &Update<Never, pkg_fetch::PackageInfo, pkg_fetch::PackageOutput>,
    ) {
        let id = update.id;
        match &update.kind {
            UpdateKind::Started(Some(info)) => {
                let progress_bar = ProgressBar::new(0)
                    .with_message(format!("{} {}", "Downloading".blue(), info.name.as_str().bold()))
                    .with_style(
                        ProgressStyle::with_template(
                            " {spinner} |{percent:>3}%| {wide_msg} {binary_bytes_per_sec:>.dim} ",
                        )
                        .unwrap()
                        .tick_chars("--=≡■≡=--"),
                    );
                let progress_bar = match self.task_progress(parent) {
                    Some(parent) => self.multi_progress.insert_before(&parent.progress_bar, progress_bar),
                    None => self.multi_progress.add(progress_bar),
                };
                progress_bar.enable_steady_tick(Duration::from_millis(150));

                self.insert(id, progress_bar, info.name.to_string());
            }
            UpdateKind::Progress { current, total, .. } => self.progress(id, *current, *total),
            UpdateKind::Completed(Some(output)) => {
                let Some(task_progress) = self.remove(id) else {
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
            UpdateKind::Aborted => {
                self.remove(id);
            }
            _ => {}
        }
    }

    fn extract_packages(
        &self,
        update: &Update<ExtractPackage, pkg_extract::PackagesInfo, Never, pkg_extract::PackagesDetail>,
    ) {
        if let UpdateKind::Detail(pkg_extract::PackagesDetail::Skipped { path }) = &update.kind {
            println!("{path:?}: No layout records found, skipping.");
        }
    }

    fn extract_package(&self, update: &Update<Never, pkg_extract::PackageInfo>) {
        if let UpdateKind::Started(Some(info)) = &update.kind {
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
