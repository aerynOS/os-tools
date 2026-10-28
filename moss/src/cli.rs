// SPDX-FileCopyrightText: 2023 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    ffi::{OsStr, OsString},
    io,
    ops::Deref,
    path::PathBuf,
};

use clap::{Args, CommandFactory, Parser};
use moss::{Installation, client, installation};
use stone::StoneReadError;
use thiserror::Error;
use tui::Styled;

mod boot;
mod cache;
mod completions;
mod index;
mod inspect;
mod pkg;
mod repo;
mod search;
mod search_file;
mod state;
mod sync;
mod tree;

pub fn run() -> Result<(), BoxedError> {
    Command::parse_expanded().run()
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

    #[error("stone format")]
    Format(#[from] StoneReadError),

    #[error("One or more files failed the integrity check")]
    ValidationFailed,

    #[error(transparent)]
    Package(#[from] pkg::Error),

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

#[derive(Debug, Parser)]
#[command(
    disable_help_subcommand = true,
    disable_version_flag = true,
    propagate_version = true,
    allow_external_subcommands = true,
    version = tools_buildinfo::get_full_version(),
    about = "aerynOS' package manager"
)]
struct Command {
    #[command(subcommand)]
    subcommand: Subcommand,
    #[command(flatten)]
    global: Global,
}

impl Command {
    pub fn parse_expanded() -> Self {
        Self::parse_expanded_from(std::env::args_os())
    }

    pub fn try_parse_expanded_from<I, T>(raw_args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let raw_args: Vec<OsString> = raw_args.into_iter().map(Into::into).collect();

        let cmd = Self::try_parse_from(raw_args.iter())?;
        let Subcommand::Alias(cmd_args) = &cmd.subcommand else {
            return Ok(cmd);
        };
        let Some(alias) = cmd_args.first() else {
            return Ok(cmd);
        };

        let Ok(alias_index) = Self::ALIASES.binary_search_by_key(&alias.as_os_str(), |(key, _)| OsStr::new(key)) else {
            return Err(Self::unknown_subcommand_error(&raw_args));
        };

        let cmd_start = raw_args
            .windows(cmd_args.len())
            .position(|window| window == cmd_args)
            .unwrap(); // There's no way `position` can fail since cmd_args is computed from raw_args.
        let raw_args = raw_args[..cmd_start]
            .iter()
            .map(|arg| arg.as_os_str())
            .chain(Self::ALIASES[alias_index].1.iter().map(OsStr::new))
            .chain(cmd_args[1..].iter().map(|arg| arg.as_os_str()));

        Self::try_parse_from(raw_args)
    }

    pub fn parse_expanded_from<I, T>(raw_args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        match Self::try_parse_expanded_from(raw_args) {
            Ok(cmd) => cmd,
            Err(err) => err.exit(),
        }
    }

    /// Builds the error for an unknown subcommand/alias.
    /// The caller must ensure no subcommand or alias matches, given
    /// the provided `raw_args`.
    ///
    /// # Panics
    ///
    /// Panics if a subcommand or alias matches
    fn unknown_subcommand_error(raw_args: &[OsString]) -> clap::Error {
        Self::command()
            .allow_external_subcommands(false)
            .external_subcommand_value_parser(clap::builder::Resettable::Reset)
            // HACK: hidden subcommands, only so similar aliases are also suggested
            .subcommands(
                Self::ALIASES
                    .iter()
                    .map(|(alias, _)| clap::Command::new(*alias).hide(true)),
            )
            .try_get_matches_from(raw_args)
            .expect_err("subcommand or alias should not match")
    }

    /// Run the CLI according to users' flags and arguments.
    pub fn run(self) -> Result<(), BoxedError> {
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
            Subcommand::Completions(cmd) => cmd.handle()?,
            Subcommand::Index(cmd) => cmd.handle()?,
            Subcommand::Inspect(cmd) => cmd.handle()?,
            Subcommand::Pkg(cmd) => cmd.handle(self.global, installation)?,
            Subcommand::Repo(cmd) => cmd.handle(installation)?,
            Subcommand::Search(cmd) => cmd.handle(installation)?,
            Subcommand::SearchFile(cmd) => cmd.handle(installation)?,
            Subcommand::State(cmd) => cmd.handle(self.global, installation)?,
            Subcommand::Sync(cmd) => cmd.handle(self.global, installation)?,
            Subcommand::Tree(cmd) => cmd.handle(),

            Subcommand::Alias(_) => unreachable!(),
        }

        Ok(())
    }

    // Keep this list sorted for the binary search.
    // Ideally we would sort this list at compile time,
    // but there's no such feature at the moment.
    const ALIASES: &[(&str, &[&str])] = &[
        ("pad", &["pkg", "add"]),
        ("pex", &["pkg", "extract"]),
        ("pfe", &["pkg", "fetch"]),
        ("pif", &["pkg", "info"]),
        ("pla", &["pkg", "list", "--available"]),
        ("ple", &["pkg", "list", "--explicit"]),
        ("pli", &["pkg", "list", "--installed"]),
        ("pls", &["pkg", "list", "--sync"]),
        ("prm", &["pkg", "remove"]),
        ("rad", &["repo", "add"]),
        ("rdi", &["repo", "disable"]),
        ("ren", &["repo", "enable"]),
        ("rls", &["repo", "list"]),
        ("rrm", &["repo", "remove"]),
        ("rup", &["repo", "update"]),
        ("sr", &["search"]),       // The least bad option (still not good)
        ("srf", &["search-file"]), // NOTE: This might go away once the search verb is properly realised
        ("sta", &["state", "activate"]),
        ("stg", &["state", "get-active"]),
        ("str", &["state", "remove"]),
        ("ste", &["state", "export"]),
        ("sti", &["state", "info"]),
        ("stl", &["state", "list"]),
        ("sts", &["state", "search"]),
        ("stv", &["state", "verify"]),
    ];
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
    version: Option<bool>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    Boot(boot::Command),
    Cache(cache::Command),
    Completions(completions::Command),
    Index(index::Command),
    #[command(about = "Show detailed (debug) information on a moss-format file")]
    Inspect(inspect::Command),
    #[command(name = "pkg", alias = "package")]
    Pkg(pkg::Command),
    #[command(name = "repo", alias = "repository")]
    Repo(repo::Command),
    Search(search::Command),
    SearchFile(search_file::Command),
    State(state::Command),
    Sync(sync::Command),
    Tree(tree::Command),

    /// This is not a real subcommand, but a hook
    /// for our alias-to-subcommands translator.
    #[command(external_subcommand)]
    Alias(Vec<OsString>),
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

    #[test]
    fn alias_retain_global_flags_head() -> Result<(), String> {
        let command = Command::try_parse_expanded_from(["moss", "-D", "/tmp", "--verbose", "pad", "mypackage"].iter())
            .map_err(|error| error.to_string())?;

        assert_eq!(command.global.root_dir, Some("/tmp".into()));
        assert!(command.global.verbose);
        assert!(matches!(command.subcommand, super::Subcommand::Pkg(_)));
        Ok(())
    }

    #[test]
    fn alias_retain_global_flags_tail() -> Result<(), String> {
        let command = Command::try_parse_expanded_from(["moss", "pad", "mypackage", "-D", "/tmp", "--verbose"].iter())
            .map_err(|error| error.to_string())?;

        assert_eq!(command.global.root_dir, Some("/tmp".into()));
        assert!(command.global.verbose);
        assert!(matches!(command.subcommand, super::Subcommand::Pkg(_)));
        Ok(())
    }

    #[test]
    fn alias_retain_global_flags_mixed() -> Result<(), String> {
        let command = Command::try_parse_expanded_from(["moss", "-D", "/tmp", "pad", "mypackage", "--verbose"].iter())
            .map_err(|error| error.to_string())?;

        assert_eq!(command.global.root_dir, Some("/tmp".into()));
        assert!(command.global.verbose);
        assert!(matches!(command.subcommand, super::Subcommand::Pkg(_)));
        Ok(())
    }
}
