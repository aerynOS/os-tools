// SPDX-FileCopyrightText: 2023 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use clap::Parser;
use moss::client::index::Error;

#[derive(Debug, Parser)]
#[command(about = "Index a collection of .stone packages")]
pub struct Command {
    #[arg(
        short,
        long = "input-dir",
        help = "The directory from which to start the index operation",
        default_value = ".",
        global = false
    )]
    input_dir: PathBuf,

    #[arg(
        short,
        long = "output-dir",
        help = "The directory to which to write the stone.index [default: INPUT_DIR]",
        global = false
    )]
    output_dir: Option<PathBuf>,
}

impl Command {
    pub fn handle(self) -> Result<(), Error> {
        // moss::client::index expects absolute paths.
        let input_dir = &self.input_dir.canonicalize()?;
        let output_dir = if let Some(out) = &self.output_dir {
            &out.canonicalize()?
        } else {
            input_dir
        };

        moss::client::index(input_dir, Some(output_dir))?;

        Ok(())
    }
}
