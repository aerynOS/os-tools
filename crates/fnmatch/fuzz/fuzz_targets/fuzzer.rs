// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

#![no_main]

use libfuzzer_sys::fuzz_target;

use fnmatch::Pattern;

fuzz_target!(|data: &[u8]| {
    let pattern_string = String::from_utf8_lossy(&data[..data.len() / 2]);
    let path = String::from_utf8_lossy(&data[data.len() / 2..]);

    let pattern = Pattern::new(pattern_string);
    pattern.matches(path);
});
