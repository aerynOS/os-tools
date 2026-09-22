// SPDX-FileCopyrightText: 2023 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::env;
use std::str::FromStr;

use crate::fstree;

pub const NAME: &str = env!("CARGO_PKG_NAME");
/// Max concurrency for disk tasks
pub const MAX_DISK_CONCURRENCY: usize = 16;
/// Max concurrency for network tasks
pub const MAX_NETWORK_CONCURRENCY: usize = 8;
/// Buffer size used when reading a file, 4 MiB
pub const FILE_READ_BUFFER_SIZE: usize = 4 * 1024 * 1024;
/// Threshold to begin chunking file during read, 16 KiB
pub const FILE_READ_CHUNK_THRESHOLD: usize = 16 * 1024;

/// Value of `MOSS_FSTREE_FORMAT`, if specified & valid
pub fn fstree_format() -> Option<fstree::Format> {
    parse_var("MOSS_FSTREE_FORMAT")
}

fn parse_var<T: FromStr>(name: &'static str) -> Option<T> {
    let var = env::var(name).ok()?;
    var.parse().ok()
}
