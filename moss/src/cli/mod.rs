// SPDX-FileCopyrightText: 2023 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{env, io, ops::Deref, path::PathBuf, slice};

use clap::{Args, CommandFactory, Parser};
use clap_complete::{
    generate_to,
    shells::{Bash, Fish, Zsh},
};
use clap_mangen::Man;
use fs_err::{self as fs, File};
use moss::{Installation, client, installation};
use thiserror::Error;
use tui::Styled;

mod boot;
mod cache;
mod index;
mod package;
mod repo;
mod search;
mod search_file;
mod state;
mod sync;

pub fn run() -> Result<(), BoxedError> {
    Command::parse_from(ExpandedArgs::from(env::args())).run()
}

/// Whether to interactively ask the
/// user for confirmation or not.
/// When [Confirmation::DoNotAsk], the operation will continue
/// without any user interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Ask,
    DoNotAsk,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("cache")]
    Cache(#[from] cache::Error),

    #[error(transparent)]
    Client(#[from] client::Error),

    #[error("index")]
    Index(#[from] client::index::Error),

    #[error(transparent)]
    Package(#[from] package::Error),

    #[error("repo")]
    Repo(#[from] repo::Error),

    #[error("search")]
    Search(#[from] search::Error),

    #[error("search-file")]
    SearchFile(#[from] search_file::Error),

    #[error("state")]
    State(#[from] state::Error),

    #[error("installation")]
    Installation(#[from] installation::Error),

    #[error("I/O error")]
    Io(#[from] io::Error),
}

pub struct BoxedError(Box<Error>);

impl<E: std::error::Error> From<E> for BoxedError
where
    Error: From<E>,
{
    fn from(value: E) -> Self {
        let error = Error::from(value);
        Self(Box::new(error))
    }
}

impl Deref for BoxedError {
    type Target = Error;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Generate the new CLI command structure
#[derive(Debug, Parser)]
#[command(
    disable_version_flag = true,
    propagate_version = true,
    version = tools_buildinfo::get_full_version(),
)]
struct Command {
    #[command(subcommand)]
    subcommand: Subcommand,
    #[command(flatten)]
    global: Global,
}

impl Command {
    /// Run the CLI according to users' flags and arguments.
    pub fn run(self) -> Result<(), BoxedError> {
        if let Some(dir) = self.global.generate_manpages {
            fs::create_dir_all(&dir)?;
            let main_cmd = Command::command();
            // Generate man page for the main command
            let main_man = Man::new(main_cmd.clone());
            let mut buffer = File::create(dir.join("moss.1"))?;
            main_man.render(&mut buffer)?;

            // Generate man pages for all subcommands
            for sub in main_cmd.get_subcommands() {
                let sub_man = Man::new(sub.clone());
                let name = format!("moss-{}.1", sub.get_name());
                let mut buffer = File::create(dir.join(&name))?;
                sub_man.render(&mut buffer)?;

                for nested in sub.get_subcommands() {
                    let nested_man = Man::new(nested.clone());
                    let name = format!("moss-{}-{}.1", sub.get_name(), nested.get_name());
                    let mut buffer = File::create(dir.join(&name))?;
                    nested_man.render(&mut buffer)?;
                }
            }
            return Ok(());
        }

        if let Some(dir) = self.global.generate_completions {
            fs::create_dir_all(&dir)?;
            let mut cmd = Command::command();
            generate_to(Bash, &mut cmd, "moss", &dir)?;
            generate_to(Fish, &mut cmd, "moss", &dir)?;
            generate_to(Zsh, &mut cmd, "moss", &dir)?;
            return Ok(());
        }

        // The default is "/" in the absence of an explicit arg.
        let installation = Installation::open(
            self.global.root_dir.clone().unwrap_or_default(),
            self.global.cache_dir.clone(),
        )?;

        if let Some(system_model) = installation.system_model.as_ref() {
            if !system_model.disable_warning {
                print_system_model_warning(&installation, false);
            } else if self.global.verbose {
                print_system_model_warning(&installation, true);
            }
        }

        match self.subcommand {
            Subcommand::Boot(cmd) => cmd.handle(installation)?,
            Subcommand::Cache(cmd) => cmd.handle(installation)?,
            Subcommand::Index(cmd) => cmd.handle()?,
            Subcommand::Package(cmd) => cmd.handle(self.global, installation)?,
            Subcommand::Repo(cmd) => cmd.handle(installation)?,
            Subcommand::Search(cmd) => cmd.handle(installation)?,
            Subcommand::SearchFile(cmd) => cmd.handle(installation)?,
            Subcommand::State(cmd) => cmd.handle(self.global, installation)?,
            Subcommand::Sync(cmd) => cmd.handle(self.global, installation)?,
        }

        Ok(())
    }
}

/// Globally available arguments
#[derive(Debug, Args)]
struct Global {
    #[arg(
        short,
        long = "verbose",
        global = true,
        help = "Prints additional information about what moss is doing",
        help_heading = "Global Options",
        default_value = "false",
        global = true
    )]
    verbose: bool,
    #[arg(
        short = 'D',
        long = "directory",
        global = true,
        help = "Root directory",
        help_heading = "Global Options",
        default_value = "/",
        global = true
    )]
    root_dir: Option<PathBuf>,
    #[arg(long, help = "Cache directory", global = true, help_heading = "Global Options")]
    cache_dir: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help = "Logging configuration: <level>[:<format>][:<destination>]\nLevels: trace, debug, info, warn, error\nFormats: text, json\nDestinations: stderr, <file>",
        help_heading = "Global Options",
        global = true
    )]
    log: Option<String>,
    #[arg(
        short = 'y',
        long = "yes",
        global = true,
        action = clap::ArgAction::SetTrue,
        help = "Assume yes for all questions",
        help_heading = "Global Options",
    )]
    confirm: Confirmation,
    #[arg(
        short = 'V',
        long = "version",
        global = true,
        action = clap::ArgAction::Version,
        help_heading = "Global Options",
        help = "Print version and exit"
    )]
    version: bool,
    #[arg(
        long = "generate-manpages",
        global = true,
        help = "Generate man pages in specified directory",
        help_heading = "Global Options",
        value_name = "DIR",
        hide = true
    )]
    generate_manpages: Option<PathBuf>,
    #[arg(
        long = "generate-completions",
        global = true,
        help = "Generate shell completions in specified directory",
        help_heading = "Global Options",
        value_name = "DIR",
        hide = true
    )]
    generate_completions: Option<String>,
}

/// Iterator that wraps [std::env::Args] to translate command aliases
/// into their expanded form.
/// If no command alias was used, this iterator yields equally to [std::env::Args].
///
/// This is needed because clap [doesn't support](https://github.com/clap-rs/clap/discussions/3672)
/// "application-level" aliases à la Git.
struct ExpandedArgs {
    args: env::Args,
    expansion: Option<slice::Iter<'static, &'static str>>,
}

impl From<env::Args> for ExpandedArgs {
    fn from(args: env::Args) -> Self {
        Self { args, expansion: None }
    }
}

impl Iterator for ExpandedArgs {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(exp) = &mut self.expansion {
            return exp.next().map_or_else(|| self.args.next(), |arg| Some(arg.to_string()));
        }
        match self.args.next() {
            Some(arg) if arg.starts_with('-') => Some(arg),
            Some(arg) => {
                let Ok(index) = Self::ALIASES.binary_search_by_key(&arg.as_str(), |&(alias, _)| alias) else {
                    return Some(arg);
                };
                self.expansion = Some(Self::ALIASES[index].1.iter());
                self.expansion.as_mut().unwrap().next().map(|arg| arg.to_string())
            }
            None => None,
        }
    }
}

impl ExpandedArgs {
    // Keep this list sorted for the binary search.
    // Ideally we would sort this list at compile time,
    // but there's no such feature at the moment.
    const ALIASES: &[(&str, &[&str])] = &[
        ("pad", &["package", "add"]),
        ("pex", &["package", "extract"]),
        ("pfe", &["package", "fetch"]),
        ("pif", &["package", "info"]),
        ("pin", &["package", "inspect"]),
        ("pls", &["package", "list"]),
        ("prm", &["package", "remove"]),
    ];
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    Boot(boot::Command),
    Cache(cache::Command),
    Index(index::Command),
    Package(package::Command),
    Repo(repo::Command),
    Search(search::Command),
    SearchFile(search_file::Command),
    State(state::Command),
    Sync(sync::Command),
}

impl From<bool> for Confirmation {
    fn from(value: bool) -> Self {
        if value {
            Confirmation::DoNotAsk
        } else {
            Confirmation::Ask
        }
    }
}

impl From<&str> for Confirmation {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "y" | "yes" | "true" => Confirmation::DoNotAsk,
            _ => Confirmation::Ask,
        }
    }
}

fn print_system_model_warning(installation: &Installation, first_line_only: bool) {
    let path = installation.system_model_path();

    eprintln!("{}: {path:?} is present & therefore active.", "INFO".green());

    if !first_line_only {
        eprintln!(
            "Hence:
- The system-model is the source of truth and defines all
  repositories & installed packages.
- Any changes made via `moss` commands will be temporary
  until the system-model is updated.
- The system state can be reverted to match the system-model state
  by doing a `moss sync`.
- To disable the system-model, remove or rename {path:?}.",
        );
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Command;

    #[test]
    fn cli_tree_is_valid() {
        let cmd: clap::Command = Command::command();
        cmd.debug_assert();
    }
}
