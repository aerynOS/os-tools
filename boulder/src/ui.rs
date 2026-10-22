// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use tui::{MultiProgress, ProgressBar, ProgressStyle};

/// Sequence of characters rendered to signal the application is
/// performing a long-running operation.
pub const PROGRESS_CHARS: &str = "■≡=- ";

/// Runs an asynchronous function against each item
/// of an iterator. Up to `jobs` functions can run at a time.
pub fn run_parallel<I, V>(
    jobs: usize,
    items: impl Iterator<Item = I>,
    op: impl AsyncFn(&I, &ProgressBar) -> V,
) -> impl Stream<Item = V> {
    let op = Arc::new(op);
    let prog_bars = MultiProgress::new();
    prog_bars.add(
        ProgressBar::new(items.size_hint().0 as u64).with_style(
            ProgressStyle::with_template("\n|{bar:20.cyan/blue}| {pos}/{len}")
                .unwrap()
                .progress_chars(PROGRESS_CHARS),
        ),
    );

    stream::iter(items)
        .map(move |item| {
            let prog_bars = prog_bars.clone();
            let op = Arc::clone(&op);
            async move {
                let pb = prog_bars.insert_from_back(1, ProgressBar::new(u64::MAX));
                pb.enable_steady_tick(Duration::from_millis(150));
                let result = op(&item, &pb).await;
                pb.finish_and_clear();
                prog_bars.remove(&pb);
                result
            }
        })
        .buffer_unordered(jobs)
}
