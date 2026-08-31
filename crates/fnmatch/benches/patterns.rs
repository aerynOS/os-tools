// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

const PATH: &str = r"/tmp/systemd-private-b6c2bb689c3946e1934fd8988b963caa-some-other-ver1longish-gibberish";

const NON_CAPTURING_REGEX: &str = r"/.?mp/systemd-.*-b6.*ca.?-.*-gibberish$";
const CAPTURING_REGEX: &str = r"/(?<one1>.?)mp/systemd-(?<any1>.*)-b6(?<any2>.*)ca(?<one2>.?)-(?<any3>.*)-gibberish$";

const NON_CAPTURING_PATTERN: &str = "/?mp/systemd-*-b6*ca?-*-gibberish";
const CAPTURING_PATTERN: &str = "/(one1:?)mp/systemd-(any1:*)-b6(any2:*)ca(one2:?)-(any3:*)-gibberish";

pub fn compile(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile");

    group.bench_function("non capturing pattern (fnmatch)", |b| {
        b.iter(|| fnmatch::Pattern::new(black_box(NON_CAPTURING_PATTERN)));
    });
    group.bench_function("capturing pattern (fnmatch)", |b| {
        b.iter(|| fnmatch::Pattern::new(black_box(CAPTURING_PATTERN)));
    });

    group.bench_function("non capturing pattern (wildmatch)", |b| {
        b.iter(|| wildmatch::WildMatch::new(black_box(NON_CAPTURING_PATTERN)));
    });

    group.bench_function("non capturing pattern (regex)", |b| {
        b.iter(|| regex::Regex::new(black_box(NON_CAPTURING_REGEX)).unwrap());
    });
    group.bench_function("capturing pattern (regex)", |b| {
        b.iter(|| regex::Regex::new(black_box(CAPTURING_REGEX)).unwrap());
    });
}

pub fn r#match(c: &mut Criterion) {
    let mut group = c.benchmark_group("match");

    let non_cap_fnmatch = fnmatch::Pattern::new(black_box(NON_CAPTURING_PATTERN));
    let cap_fnmatch = fnmatch::Pattern::new(black_box(CAPTURING_PATTERN));
    let non_cap_wildmatch = wildmatch::WildMatch::new(black_box(NON_CAPTURING_PATTERN));
    let non_cap_regex = regex::Regex::new(black_box(NON_CAPTURING_REGEX)).unwrap();
    let cap_regex = regex::Regex::new(black_box(CAPTURING_REGEX)).unwrap();

    group.bench_function("non capturing pattern (fnmatch)", |b| {
        b.iter(|| assert!(non_cap_fnmatch.matches(black_box(PATH)).is_some()));
    });
    group.bench_function("capturing pattern (fnmatch)", |b| {
        b.iter(|| assert!(cap_fnmatch.matches(black_box(PATH)).is_some()));
    });

    group.bench_function("non capturing pattern (wildmatch)", |b| {
        b.iter(|| assert!(non_cap_wildmatch.matches(black_box(PATH))));
    });

    group.bench_function("non capturing pattern (regex)", |b| {
        b.iter(|| assert!(non_cap_regex.is_match(black_box(PATH))));
    });
    group.bench_function("capturing pattern (regex)", |b| {
        b.iter(|| assert!(cap_regex.is_match(black_box(PATH))));
    });
}

criterion_group!(benches, compile, r#match);
criterion_main!(benches);
