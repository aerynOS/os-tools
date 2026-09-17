// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use moss::completions;
use moss::package::Flags;
use moss::{Client, Installation};

fn criterion_benchmark(c: &mut Criterion) {
    // Use actual moss database for benchmarks
    let installation = match Installation::open("/", None) {
        Ok(installation) => installation,
        Err(err) => {
            eprintln!("Skipping completions benchmark: {err}");
            return;
        }
    };
    let client = match Client::new("moss", installation) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Skipping completions benchmark: {err}");
            return;
        }
    };

    let flags = Flags::default().with_available();
    let prefixes = &["a", "g", "l", "lib", "p", "py"];
    let mut group = c.benchmark_group("prefix_completion");
    for prefix in prefixes {
        group.bench_with_input(BenchmarkId::new("available", prefix), prefix, |b, &p| {
            b.iter(|| completions::generate_results(&client, flags, black_box(p)));
        });
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
