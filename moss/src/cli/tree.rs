// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::fmt::{self, Write};
use std::iter;

use clap::CommandFactory;

use crate::cli;

#[derive(Debug, clap::Parser)]
pub struct Command {}

impl Command {
    pub fn handle(&self) {
        let mut output = String::new();
        let root = cli::Command::command();

        render_subcommand(&mut output, 0, &root).unwrap();
        print!("{output}");
    }
}

fn render_subcommand(buf: &mut String, depth: usize, subcmd: &clap::Command) -> Result<(), fmt::Error> {
    render_indentation(buf, depth)?;
    if let Some(about) = subcmd.get_about() {
        writeln!(buf, "{} - {about}", subcmd.get_name())?;
    } else {
        writeln!(buf, "{}", subcmd.get_name())?;
    }

    for sub in subcmd.get_subcommands() {
        render_subcommand(buf, depth + 1, sub)?;
    }

    for arg in subcmd.get_arguments() {
        render_indentation(buf, depth + 1)?;
        if let Some(short) = arg.get_short() {
            write!(buf, "-{short}")?;
            if arg.get_long().is_some() {
                write!(buf, ", ")?;
            }
        }
        if let Some(long) = arg.get_long() {
            write!(buf, "--{long}")?;
        }
        if arg.get_short().is_none() && arg.get_long().is_none() {
            write!(buf, "{}", arg.get_id())?;
        }
        if let Some(help) = arg.get_help() {
            write!(buf, "\t{help}")?;
        }
        writeln!(buf)?;
    }

    Ok(())
}

fn render_indentation(buf: &mut String, depth: usize) -> Result<(), fmt::Error> {
    write!(buf, "{}", iter::repeat_n("│  ", depth).collect::<String>())?;
    Ok(())
}
