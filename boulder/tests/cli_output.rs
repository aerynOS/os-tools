// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::Path;

#[test]
fn cli_output() {
    cli_expected::check(
        "boulder",
        env!("CARGO_BIN_EXE_boulder"),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expected"),
    );
}
