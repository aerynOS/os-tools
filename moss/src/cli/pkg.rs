// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{io, path::PathBuf};

use clap::Parser;
use info::info;
use list::list;
use moss::{Client, Installation, client, dependency::Provider, environment};
use stone::StoneReadError;
use thiserror::Error;
use tracing::instrument;

mod info;
mod list;

use crate::cli::{Confirmation, Global};

#[derive(Debug, Parser)]
#[command(about = "Manage, list, or show packages")]
pub struct Command {
    #[command(subcommand)]
    subcommand: Subcommand,
}

impl Command {
    /// Handles the "package" subcommand.
    pub fn handle(self, global: Global, installation: Installation) -> Result<(), Error> {
        match self.subcommand {
            Subcommand::Fetch { output_dir, packages } => fetch(global, output_dir, packages, installation),
            Subcommand::Add(args) => add(global, args, installation),
            Subcommand::Info(args) => info(args, installation),
            Subcommand::List(args) => list(args, installation),
            Subcommand::Extract { files, output_dir } => extract(files, output_dir),
            Subcommand::Remove(args) => remove(global, args, installation),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("No such package {0}")]
    NotFound(String),
    #[error("No packages found")]
    NoneFound,
    #[error("client")]
    Client(#[from] client::Error),
    #[error("stone format")]
    Format(#[from] StoneReadError),
    #[error(transparent)]
    Extract(#[from] client::extract::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    #[command(about = "Fetch package stone(s) by name or provider [aliases: pfe]")]
    Fetch {
        /// directory to write the fetched stone(s)
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,

        /// packages to fetch
        #[arg(name = "PACKAGE", required = true)]
        packages: Vec<Provider>,
    },

    #[command(about = "Add package(s) to the implicit system-model [aliases: pad]")]
    Add(AddArgs),

    #[command(about = "Show information about the package provider [aliases: pif]")]
    Info(InfoArgs),

    #[command(about = "List packages according to the supplied filter argument (requires an argument)")]
    List(ListArgs),

    #[command(about = "Extract contents of `.stone` archive(s) to disk [aliases: pex]")]
    Extract {
        #[arg(help = "valid moss-format archives(s) to extract")]
        files: Vec<PathBuf>,

        #[arg(
            short,
            long = "output-dir",
            help = "directory to which to extract moss-format archive(s) (default: `.`)",
            default_value = "."
        )]
        output_dir: PathBuf,
    },

    #[command(about = "Remove package(s) from the implicit system-model [aliases: prm]")]
    Remove(RemoveArgs),
}

#[derive(Clone, Debug, clap::Args)]
struct AddArgs {
    #[arg(help = "Package name(s) or provider(s) to add to the system-model")]
    providers: Vec<Provider>,
    #[arg(long)]
    reinstall: bool,
    #[arg(long, help = "Simulate the operation")]
    dry_run: bool,
    /// Blit this sync to the provided directory instead of the root.
    ///
    /// This operation won't be captured as a new state.
    #[arg(value_name = "dir", long = "to")]
    blit_target: Option<PathBuf>,
}

#[derive(Clone, Debug, clap::Args)]
struct InfoArgs {
    provider: Provider,
    #[arg(short, long, help = "Filter to list of known repositories")]
    repositories: Vec<String>,
    #[arg(short = 'f', long = "show-files", help = "Show files provided by package")]
    show_files: bool,
}

#[derive(Clone, Debug, clap::Args)]
#[group(required = true)]
struct ListArgs {
    #[arg(
        short,
        long,
        help = "List available packages given the active system-model repositories [aliases: pla]"
    )]
    available: bool,
    #[arg(
        short,
        long,
        help = "List explicitly added packages in the system-model [aliases: ple]"
    )]
    explicit: bool,
    #[arg(
        short,
        long,
        help = "List the resolved set of packages installed from the system-model [aliases: pli]"
    )]
    installed: bool,
    #[arg(
        short,
        long,
        help = "Limit the list of packages to the specified, known repositories"
    )]
    repositories: Vec<String>,
    #[arg(
        short,
        long,
        help = "List the packages that would be sync'd based on the cached repository state [aliases: pls]"
    )]
    sync: bool,
}

#[derive(Clone, Debug, clap::Args)]
struct RemoveArgs {
    #[arg(help = "Package name(s) or provider(s) to remove from the system-model")]
    providers: Vec<Provider>,
    #[arg(long, help = "Simulate the operation")]
    dry_run: bool,
}

/// Handle execution of `moss pkg fetch`
#[instrument(skip_all)]
fn fetch(
    global: Global,
    output_dir: PathBuf,
    providers: Vec<Provider>,
    installation: Installation,
) -> Result<(), Error> {
    let pkgs = providers.iter().map(|prov| prov.to_string()).collect::<Vec<_>>();

    let mut client = Client::new(environment::NAME, installation)?;

    // FIXME: Maybe we want client.fetch to accept Providers?
    // It's already using them internally.
    let pkgs_str = pkgs.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    client.fetch(&pkgs_str, &output_dir, global.verbose)?;

    Ok(())
}

/// Handle execution of `moss pkg add`
#[instrument(skip_all)]
fn add(global: Global, args: AddArgs, installation: Installation) -> Result<(), Error> {
    let pkgs = args.providers.iter().map(|prov| prov.to_string()).collect::<Vec<_>>();

    // Grab a client for the root
    let mut client = Client::new(environment::NAME, installation)?;

    // Make ephemeral if a blit target was provided
    if let Some(blit_target) = args.blit_target {
        client = client.ephemeral(blit_target)?;
    }

    // FIXME: Maybe we want client.install to accept Providers?
    // It's already using them internally.
    let pkgs_str = pkgs.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    client.install(&pkgs_str, global.confirm == Confirmation::DoNotAsk, args.dry_run)?;

    Ok(())
}

/// Handle execution of `moss pkg remove`
#[instrument(skip_all)]
fn remove(global: Global, args: RemoveArgs, installation: Installation) -> Result<(), Error> {
    let pkgs = args.providers.iter().map(|p| p.to_string()).collect::<Vec<_>>();

    let mut client = Client::new(environment::NAME, installation)?;

    // FIXME: Maybe we want client.remove to accept Providers?
    // It's already using them internally.
    let pkgs_str = pkgs.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    client.remove(&pkgs_str, global.confirm == Confirmation::DoNotAsk, args.dry_run)?;

    Ok(())
}

/// Handle execution of `moss pkg extract`
fn extract(files: Vec<PathBuf>, output_dir: PathBuf) -> Result<(), Error> {
    let paths_ref = files.iter().collect();
    client::extract(paths_ref, &output_dir)?;
    Ok(())
}
