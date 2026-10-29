// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Checks the output of CLI commands against expected output stored in files.
//!
//! Setting the "UPDATE_EXPECTED" env var to a command (e.g. `moss pad`) writes
//! the output of that command to a file, creating the file if needed.
//! Setting it to `*` rewrites all existing files.
//!
//! If "UPDATE_EXPECTED" is empty, all outputs are tested against their expected files.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use fs_err as fs;
use similar::TextDiff;

const COMMAND_PREFIX: &str = "$ ";

/// Runs all commands from the expected files in `dir` with the binary `bin_path`
/// and panics if any output differs from what is expected.
///
/// `bin_name` is the name the command that the expected file starts with.
pub fn check(bin_name: &str, bin_path: &str, dir: &Path) {
    match std::env::var("UPDATE_EXPECTED") {
        Ok(update) if update == "*" => {
            for (path, command) in expected_command_outputs(dir) {
                write_expected(&path, &command, bin_path);
            }
        }
        Ok(update) => {
            let command = update.split_whitespace().collect::<Vec<_>>().join(" ");
            if command.split(' ').next() == Some(bin_name) {
                let fp = dir.join(expected_file_name(&command));
                write_expected(&fp, &command, bin_path);
            }
        }
        Err(_) => {
            let mut failures = String::new();

            for (path, command) in expected_command_outputs(dir) {
                let expected = fs::read_to_string(&path).unwrap();
                let actual = run_command(&command, bin_path);

                if expected != actual {
                    let diff = TextDiff::from_lines(&expected, &actual);
                    let _ = write!(
                        failures,
                        "\n{}\n{}\nrun `just update-test-expected \"{command}\"` if this change is intended\n",
                        path.display(),
                        diff.unified_diff().header("expected", "actual"),
                    );
                }
            }

            assert!(failures.is_empty(), "command output differs from expected:\n{failures}");
        }
    }
}

/// Returns all expected files in `dir`, along with the corresponding command
fn expected_command_outputs(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut files = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .map(|path| {
            let contents = fs::read_to_string(&path).unwrap();
            // first line should be the command of format `$ <command>`
            let command = contents
                .lines()
                .next()
                .and_then(|line| line.strip_prefix(COMMAND_PREFIX))
                .unwrap_or_else(|| panic!("{} does not start with `{COMMAND_PREFIX}<command>`", path.display()))
                .to_owned();
            (path, command)
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Derives the expected file name from the command, e.g. `moss pad` -> `moss_pad.txt`
fn expected_file_name(command: &str) -> String {
    let stem = command
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect::<String>();
    format!("{stem}.txt")
}

fn write_expected(path: &Path, command: &str, bin_path: &str) {
    fs::write(path, run_command(command, bin_path)).unwrap();
}

/// Runs `command` with `bin_path` and "renders" its output
fn run_command(command: &str, bin_path: &str) -> String {
    let Output { status, stdout, stderr } = Command::new(bin_path)
        .args(command.split(' ').skip(1))
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|err| panic!("failed to run `{command}`: {err}"));

    let status = status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| status.to_string());

    format!(
        "{COMMAND_PREFIX}{command}\n--- exit status: {status}\n--- stdout\n{}--- stderr\n{}",
        terminated(&stdout),
        terminated(&stderr),
    )
}

fn terminated(output: &[u8]) -> String {
    let mut output = String::from_utf8_lossy(output).into_owned();
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output
}
