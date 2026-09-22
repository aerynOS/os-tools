// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use fs_err as fs;
use moss::client::Error;

#[derive(Clone, Debug, Parser)]
#[command(about = "Generate shell completions and man pages")]
pub struct Command {
    #[arg(help = "Destination directory")]
    directory: PathBuf,
}

impl Command {
    pub fn handle(self) -> Result<(), Error> {
        let mut cmd = <super::Command as CommandFactory>::command();
        let cmd_name = env!("CARGO_PKG_NAME");
        fs::create_dir_all(&self.directory)?;

        clap_mangen::generate_to(cmd.clone(), &self.directory)?;

        for shell in [Shell::Bash, Shell::Fish, Shell::Zsh] {
            clap_complete::generate_to(shell, &mut cmd, cmd_name, &self.directory)?;
        }

        Ok(())
    }
}
