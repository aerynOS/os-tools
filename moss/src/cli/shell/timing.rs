// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use moss::{
    Event,
    event::{
        self, Update, UpdateKind,
        pkg_extract::{ExtractPackage, PkgExtract},
        pkg_fetch::{FetchPackage, PkgFetch},
        task,
    },
};
use tui::Styled;

/// Records how long each task took, to report as a tree once the command finishes
#[derive(Default)]
pub struct Timing {
    tasks: Mutex<Tasks>,
}

#[derive(Default)]
struct Tasks {
    nodes: Vec<Node>,
    index: HashMap<task::Id, usize>,
}

struct Node {
    id: task::Id,
    parent: Option<task::Id>,
    name: &'static str,
    label: String,
    started: Instant,
    finished: Option<(Instant, End)>,
}

#[derive(Clone, Copy)]
enum End {
    Completed,
    Aborted,
}

impl event::Subscriber for Timing {
    fn event(&self, event: &Event) {
        match event {
            Event::PkgFetch(Update {
                kind: UpdateKind::Subtask { subtask, .. },
                ..
            }) => self.pkg_fetch(subtask),
            Event::PkgExtract(Update {
                kind: UpdateKind::Subtask { subtask, .. },
                ..
            }) => self.pkg_extract(subtask),
            // FIXME: Do something about `UpdateKind::Started`
            _ => {}
        }
    }
}

impl Timing {
    fn pkg_fetch(&self, event: &PkgFetch) {
        match event {
            PkgFetch::FetchPackages(Update {
                kind:
                    UpdateKind::Subtask {
                        subtask: FetchPackage(update),
                        ..
                    },
                ..
            }) => {
                self.task(update, "fetch_package", |task| task.name.to_string());
            }
            PkgFetch::FetchPackages(update) => {
                self.task(update, "fetch_packages", |task| format!("{} packages", task.count));
            }
        }
    }

    fn pkg_extract(&self, event: &PkgExtract) {
        match event {
            PkgExtract::ExtractPackages(Update {
                kind:
                    UpdateKind::Subtask {
                        subtask: ExtractPackage(update),
                        ..
                    },
                ..
            }) => {
                self.task(update, "extract_package", |task| task.path.display().to_string());
            }
            PkgExtract::ExtractPackages(update) => {
                self.task(update, "extract_packages", |task| format!("{} packages", task.count));
            }
        }
    }

    fn task<S, I, O, D>(&self, update: &Update<S, I, O, D>, name: &'static str, label: impl FnOnce(&I) -> String) {
        let now = Instant::now();
        let mut tasks = self.tasks.lock().unwrap();

        let id = update.id;
        match &update.kind {
            UpdateKind::Started {
                parent,
                info: Some(info),
            } => {
                let index = tasks.nodes.len();
                tasks.nodes.push(Node {
                    id,
                    parent: *parent,
                    name,
                    label: label(info),
                    started: now,
                    finished: None,
                });
                tasks.index.insert(id, index);
            }
            UpdateKind::Completed { .. } => tasks.finish(id, now, End::Completed),
            UpdateKind::Aborted => tasks.finish(id, now, End::Aborted),
            _ => {}
        }
    }

    /// Print the task tree with durations to stderr, so it never mixes with stdout
    pub fn report(&self) {
        let tasks = self.tasks.lock().unwrap();

        let mut lines = vec![];
        for (index, node) in tasks.nodes.iter().enumerate() {
            if node.parent.is_none() {
                tasks.lines(index, "", "", &mut lines);
            }
        }

        let width = lines
            .iter()
            .map(|(text, _)| text.chars().count())
            .max()
            .unwrap_or_default();
        let timing_width = lines.iter().map(|(_, timing)| timing.len()).max().unwrap_or_default();

        eprintln!("\n{}", "Timing".bold());
        for (text, timing) in lines {
            let padding = width - text.chars().count();
            eprintln!(
                "{text}{}  {}",
                " ".repeat(padding),
                format!("{timing:>timing_width$}").dim()
            );
        }
    }
}

impl Drop for Timing {
    fn drop(&mut self) {
        self.report();
    }
}

impl Tasks {
    fn finish(&mut self, id: task::Id, now: Instant, end: End) {
        if let Some(&index) = self.index.get(&id) {
            self.nodes[index].finished = Some((now, end));
        }
    }

    fn lines(&self, index: usize, prefix: &str, child_prefix: &str, lines: &mut Vec<(String, String)>) {
        let node = &self.nodes[index];

        let timing = match node.finished {
            Some((finished, End::Completed)) => format_duration(finished - node.started),
            Some((finished, End::Aborted)) => format!("{} (aborted)", format_duration(finished - node.started)),
            None => "(unfinished)".to_owned(),
        };
        lines.push((format!("{prefix}{} {}", node.name, node.label), timing));

        let children = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, child)| child.parent == Some(node.id))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();

        for (position, child) in children.iter().enumerate() {
            let last = position == children.len() - 1;
            let (branch, continuation) = if last { ("└─ ", "   ") } else { ("├─ ", "│  ") };
            self.lines(
                *child,
                &format!("{child_prefix}{branch}"),
                &format!("{child_prefix}{continuation}"),
                lines,
            );
        }
    }
}

fn format_duration(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{:.1}ms", duration.as_secs_f64() * 1_000.0)
    } else {
        format!("{:.2}s", duration.as_secs_f64())
    }
}
