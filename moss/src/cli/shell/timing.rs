// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use moss::{
    Event,
    event::{self, task},
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

impl event::Observer for Timing {
    fn event(&self, event: &Event) {
        match event {
            Event::Task(task::Event::FetchPackages(update)) => {
                self.task(update, "fetch_packages", |task| format!("{} packages", task.count));
            }
            Event::Task(task::Event::FetchPackage(update)) => {
                self.task(update, "fetch_package", |task| task.name.to_string());
            }
            Event::Task(task::Event::IndexPackages(update)) => {
                self.task(update, "index_packages", |task| format!("{} files", task.count));
            }
            Event::Task(task::Event::IndexPackage(update)) => {
                self.task(update, "index_package", |task| task.path.display().to_string());
            }
            Event::Task(task::Event::ExtractPackages(update)) => {
                self.task(update, "extract_packages", |task| format!("{} packages", task.count));
            }
            Event::Task(task::Event::ExtractPackage(update)) => {
                self.task(update, "extract_package", |task| task.path.display().to_string());
            }
            Event::Task(task::Event::Blit(update)) => {
                self.task(update, "blit", |task| format!("{} entries", task.total));
            }
        }
    }
}

impl Timing {
    fn task<T: event::Task>(&self, update: &task::Update<T>, name: &'static str, label: impl FnOnce(&T) -> String) {
        let now = Instant::now();
        let mut tasks = self.tasks.lock().unwrap();

        match update {
            task::Update::Started { id, parent, task } => {
                let index = tasks.nodes.len();
                tasks.nodes.push(Node {
                    id: *id,
                    parent: *parent,
                    name,
                    label: label(task),
                    started: now,
                    finished: None,
                });
                tasks.index.insert(*id, index);
            }
            task::Update::Completed { id, .. } => tasks.finish(*id, now, End::Completed),
            task::Update::Aborted { id } => tasks.finish(*id, now, End::Aborted),
            task::Update::Phase { .. } | task::Update::Progress { .. } | task::Update::Detail { .. } => {}
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
