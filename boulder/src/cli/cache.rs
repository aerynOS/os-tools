// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    io,
    path::{Path, PathBuf},
};

use clap::{Args, Parser};
use container::Container;
use humansize::BINARY;
use moss::util;
use rayon::iter::{ParallelBridge, ParallelIterator};
use thiserror::Error;
use walkdir::WalkDir;

use crate::Env;

#[derive(Debug, Parser)]
#[command(about = "Manage boulder caches")]
pub struct Command {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    subcommand: Subcommand,
}

#[derive(Debug, Args)]
pub struct Global {
    #[arg(
        short,
        long,
        help = "Clean the boulder cache",
        default_value_t = false,
        global = true
    )]
    pub boulder_cache: bool,
    #[arg(short, long, help = "Clean the moss cache", default_value_t = false, global = true)]
    pub moss_cache: bool,
}

#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
    #[command(about = "Clean out the cache(s) for the current environment")]
    Clean,
    #[command(about = "Show the cache size(s) for the current environment")]
    Size,
}

pub fn handle(command: Command, env: Env) -> Result<(), Error> {
    let boulder_cache = command.global.boulder_cache;
    let moss_cache = command.global.moss_cache;

    match command.subcommand {
        Subcommand::Clean => clean(env, boulder_cache, moss_cache),
        Subcommand::Size => size(env, boulder_cache, moss_cache),
    }
}

fn clean(env: Env, boulder_cache: bool, moss_cache: bool) -> Result<(), Error> {
    for (name, path) in selected_caches(&env, boulder_cache, moss_cache) {
        let tmpdir = tempfile::tempdir()?;

        println!("Deleting {name} directory: {}", path.display());

        Container::new(tmpdir.path())
            .bind_rw(&path, Path::new("/remove"))
            .run(|| util::par_remove_dir_all(Path::new("/remove")))?;
    }
    Ok(())
}

fn size(env: Env, boulder_cache: bool, moss_cache: bool) -> Result<(), Error> {
    for (name, path) in selected_caches(&env, boulder_cache, moss_cache) {
        let size: u64 = WalkDir::new(&path)
            .into_iter()
            .par_bridge()
            .filter_map(Result::ok)
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();
        println!("{name} ({}): {}", path.display(), humansize::format_size(size, BINARY));
    }
    Ok(())
}

fn selected_caches(env: &Env, boulder_cache: bool, moss_cache: bool) -> Vec<(&'static str, PathBuf)> {
    let select_all = !boulder_cache && !moss_cache;
    let mut v = Vec::new();
    if select_all || boulder_cache {
        v.push(("boulder cache", env.cache_dir.to_owned()));
    }
    if select_all || moss_cache {
        v.push(("moss cache", env.moss_dir.to_owned()));
    }
    v
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("container")]
    Container(#[from] container::Error),
    #[error("moss installation")]
    MossInstallation(#[from] moss::installation::Error),
    #[error("io")]
    Io(#[from] io::Error),
}
