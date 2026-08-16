// SPDX-FileCopyrightText: 2024 AerynOS Developers
// SPDX-FileCopyrightText: 2024 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Boot management integration in moss

use std::{
    io,
    path::{Path, PathBuf},
    str::FromStr,
    vec,
};

use blsforme::{
    Entry, Schema,
    bootloader::systemd_boot,
    os_release::{self, OsRelease},
};
use fnmatch::Pattern;
use fs_err as fs;
use itertools::Itertools;
use stone::{StonePayloadLayoutFile, StonePayloadLayoutRecord};
use thiserror::{self, Error};

use crate::{
    Installation, State, db,
    fstree::{self, Fstree},
    package::Id,
    state,
};

use super::Client;

#[derive(Debug, Error)]
pub enum Error {
    #[error("blsforme")]
    Blsforme(#[from] blsforme::Error),

    #[error("sd_boot")]
    SdBoot(#[from] systemd_boot::interface::Error),

    #[error("layoutdb")]
    Client(#[from] db::Error),

    #[error("io")]
    IO(#[from] io::Error),

    #[error("os_info")]
    OsInfo(#[from] os_info::Error),

    #[error("os_release")]
    OsRelease(#[from] os_release::Error),

    /// fnmatch pattern compilation for boot, etc.
    #[error("fnmatch pattern")]
    Pattern(#[from] fnmatch::Error),

    #[error("incomplete kernel tree")]
    IncompleteKernel(String),

    #[error("failed to find archived fstree for state {0}")]
    NoArchivedState(state::Id),

    #[error("fstree")]
    Fstree(#[from] fstree::DriverError),
}

/// Simple mapping type for kernel discovery paths, retaining the layout reference
#[derive(Debug)]
struct KernelCandidate {
    path: PathBuf,
    _layout: StonePayloadLayoutRecord,
}

impl AsRef<Path> for KernelCandidate {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}

/// From a given set of input paths, produce a set of match pairs
/// This is applied against the given system root
fn kernel_files_from_state<'a>(
    layouts: &'a [(Id, StonePayloadLayoutRecord)],
    pattern: &'a Pattern,
) -> Vec<KernelCandidate> {
    let mut kernel_entries = vec![];

    for (_, path) in layouts.iter() {
        match &path.file {
            StonePayloadLayoutFile::Regular(_, target) if pattern.match_path(target).is_some() => {
                kernel_entries.push(KernelCandidate {
                    path: PathBuf::from("usr").join(target),
                    _layout: path.to_owned(),
                });
            }
            StonePayloadLayoutFile::Symlink(_, target) if pattern.match_path(target).is_some() => {
                kernel_entries.push(KernelCandidate {
                    path: PathBuf::from("usr").join(target),
                    _layout: path.to_owned(),
                });
            }
            _ => {}
        }
    }

    kernel_entries
}

/// Find bootloader assets in the new state
fn boot_files_from_new_state<'a>(
    install: &Installation,
    layouts: &'a [(Id, StonePayloadLayoutRecord)],
    pattern: &'a Pattern,
) -> Vec<PathBuf> {
    let mut rets = vec![];

    for (_, path) in layouts.iter() {
        if let StonePayloadLayoutFile::Regular(_, target) = &path.file
            && pattern.match_path(target).is_some()
        {
            rets.push(install.root.join("usr").join(target));
        }
    }

    rets
}

/// Grab all layouts for the provided state, mapped to package id
fn layouts_for_state(client: &Client, state: &State) -> Result<Vec<(Id, StonePayloadLayoutRecord)>, db::Error> {
    client.layout_db.query(state.selections.iter().map(|s| &s.package))
}

/// Return an additional 4 older states excluding the current state
fn states_except_new<'a>(client: &'a Client, state: &State) -> Result<Vec<StateEntry<'a>>, Error> {
    let states = client
        .state_db
        .list_ids()?
        .into_iter()
        .filter_map(|(id, whence)| {
            // All states with older ID and not the current state
            if id != state.id && state.id > id {
                Some((id, whence))
            } else {
                None
            }
        })
        .sorted_by_key(|(_, whence)| whence.to_owned())
        .rev()
        .take(4)
        .rev()
        .map(|(id, _)| {
            let state = client.state_db.get(id)?;
            let fstree = client
                .open_archived_state(&state.id)
                .map_err(|_| Error::NoArchivedState(id))?;
            Ok(StateEntry::Archived { state, fstree })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(states)
}

/// Generate a schema for the root
fn os_schema_for_root(root: &Path) -> Result<Schema, Error> {
    let os_info_path = root.join("usr").join("lib").join("os-info.json");
    let os_release_path = root.join("usr").join("lib").join("os-release");

    if os_info_path.exists() {
        let info = os_info::load_os_info_from_path(&os_info_path)?;
        Ok(Schema::OsInfo {
            os_info: Box::new(info),
        })
    } else {
        let os_release = fs::read_to_string(os_release_path)?;
        let os_release = OsRelease::from_str(&os_release)?;
        Ok(Schema::Blsforme {
            os_release: Box::new(os_release),
        })
    }
}

pub fn synchronize(client: &Client, state: &State) -> Result<(), Error> {
    let root = client.installation.root.clone();
    let is_native = root.to_string_lossy() == "/";
    // Create an appropriate configuration
    let config = blsforme::Configuration {
        root: if is_native {
            blsforme::Root::Native(root.clone())
        } else {
            blsforme::Root::Image(root.clone())
        },
        vfs: "/".into(),
    };

    // For the new/active state
    let head_layouts = layouts_for_state(client, state)?;
    let kernel_pattern = Pattern::from_str("lib/kernel/(version:*)/*")?;
    let systemd = Pattern::from_str("lib*/systemd/boot/efi/*.efi")?;
    let booty_bits = boot_files_from_new_state(&client.installation, &head_layouts, &systemd);

    // no fun times without a bootloder
    if booty_bits.is_empty() {
        return Ok(());
    }

    let mut all_states = states_except_new(client, state)?;
    all_states.push(StateEntry::Active(state.clone()));

    // Ensure all archived fstree are brought up so we can read from them
    for entry in all_states.iter_mut() {
        if let StateEntry::Archived { fstree, .. } = entry {
            fstree.bring_up(fstree::Mutability::ReadOnly)?;
        }
    }

    // Always remember to bring down again
    let result = (|| -> Result<(), Error> {
        let global_schema = os_schema_for_root(&root)?;

        // Grab the entries for the new state
        let mut all_kernels = vec![];
        for state in all_states.iter() {
            let layouts = layouts_for_state(client, state.state())?;
            let local_kernels = kernel_files_from_state(&layouts, &kernel_pattern);
            let mapped = global_schema.discover_system_kernels(local_kernels.into_iter())?;
            all_kernels.push((mapped, state));
        }

        // pipe all of our entries into blsforme
        let entries = all_kernels
            .iter()
            .flat_map(|&(ref kernels, state_entry)| {
                let rootref = &root;
                let configref = &config;
                kernels.iter().filter_map(move |k| {
                    let state_id = state_entry.state().id;
                    let sysroot = match state_entry {
                        StateEntry::Active(_) => rootref.clone(),
                        StateEntry::Archived { fstree, .. } => fstree.path.clone(),
                    };

                    if !sysroot.exists() {
                        return None;
                    }

                    let local_schema = os_schema_for_root(&sysroot).ok();
                    let entry = Entry::new(configref, k)
                        .with_cmdline(format!("moss.fstx={state_id}"))
                        .with_state_id(i32::from(state_id))
                        .with_sysroot(sysroot);

                    match local_schema {
                        Some(schema) => Some(entry.with_schema(schema)),
                        None => Some(entry),
                    }
                })
            })
            .collect::<Vec<_>>();

        // no usable entries, lets get out of here.
        if entries.is_empty() {
            return Ok(());
        }

        // If we can't get a manager, find, but don't bomb. Its probably a topology failure.
        let manager = match blsforme::Manager::new(&config) {
            Ok(m) => m.with_entries(entries).with_bootloader_assets(booty_bits),
            Err(_) => return Ok(()),
        };

        // Only allow mounting pre-sync for a native run
        if is_native {
            let _mounts = manager.mount_partitions()?;
            manager.sync(&global_schema)?;
        } else {
            manager.sync(&global_schema)?;
        }

        Ok(())
    })();

    // And finally bring all archived states back down
    for entry in all_states.iter_mut() {
        if let StateEntry::Archived { fstree, .. } = entry {
            fstree.bring_down()?;
        }
    }

    result
}

pub fn print_status(installation: &Installation) -> Result<(), Error> {
    fn display_optional_path(path: Option<&Path>) -> std::path::Display<'_> {
        path.unwrap_or_else(|| "none".as_ref()).display()
    }

    let root = &installation.root;
    let is_native = root == Path::new("/");
    let config = blsforme::Configuration {
        root: if is_native {
            blsforme::Root::Native(root.clone())
        } else {
            blsforme::Root::Image(root.clone())
        },
        vfs: "/".into(),
    };

    let manager = blsforme::Manager::new(&config)?;
    match manager.boot_environment().firmware {
        blsforme::Firmware::Uefi => {
            let esp = display_optional_path(manager.boot_environment().esp());
            let xbootldr = display_optional_path(manager.boot_environment().xbootldr());
            println!("ESP            : {esp}");
            println!("XBOOTLDR       : {xbootldr}");
            if is_native && let Ok(bootloader) = systemd_boot::interface::BootLoaderInterface::new(&config.vfs) {
                let v = bootloader.get_ucs2_string(systemd_boot::interface::VariableName::Info)?;
                println!("Bootloader     : {v}");
            }
        }
        blsforme::Firmware::Bios => {
            let boot = display_optional_path(manager.boot_environment().boot_partition());
            println!("BOOT           : {boot}");
        }
    }

    println!("Global cmdline : {:?}", manager.global_cmdline());

    Ok(())
}

enum StateEntry<'a> {
    Active(State),
    Archived { state: State, fstree: Fstree<'a> },
}

impl StateEntry<'_> {
    fn state(&self) -> &State {
        match self {
            StateEntry::Active(state) => state,
            StateEntry::Archived { state, .. } => state,
        }
    }
}
