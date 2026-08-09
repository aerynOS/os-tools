// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use clap_complete::CompletionCandidate;
use itertools::Itertools;
use std::path::PathBuf;

use crate::{Installation, client, package};

const MAX_RESULTS: usize = 100;

fn generate_results(client: &client::Client, flags: package::Flags, prefix: &str) -> Vec<CompletionCandidate> {
    client
        .search_packages(prefix, flags)
        .filter(|pkg| pkg.meta.name.as_str().starts_with(prefix))
        .map(|pkg| pkg.meta.name.to_string())
        .unique()
        .take(MAX_RESULTS)
        .map(CompletionCandidate::new)
        .collect()
}

fn default_client() -> Result<client::Client, client::Error> {
    let root = PathBuf::from("/");
    let installation = Installation::open(root, None)?;
    client::Client::new("moss", installation)
}

pub fn prefix_completer(flags: package::Flags) -> impl Fn(&std::ffi::OsStr) -> Vec<CompletionCandidate> {
    move |prefix: &std::ffi::OsStr| {
        let Some(prefix) = prefix.to_str() else {
            return vec![];
        };
        if prefix.is_empty() {
            return vec![];
        }
        let Ok(client) = default_client() else {
            return vec![];
        };
        generate_results(&client, flags, prefix)
    }
}
