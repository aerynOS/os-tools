// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Drivers for creating portable filesystem trees (`fstree`) from _virtual_
//! fstrees ([`vfs::Tree`]) and their backing content (`CAS` / content address store).

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fmt, path::Path};

use astr::AStr;
use fs_err as fs;
use stone::{StonePayloadLayoutFile, StonePayloadLayoutRecord};
use thiserror::Error;

use crate::{Installation, package, util};

pub use self::native::NativeDriver;
pub use self::overlayimg::OverlayimgDriver;

pub mod native;
pub mod overlayimg;

/// A specific `fstree` format supported by `moss`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Format {
    /// An `fstree` backed by the native filesystem, using
    /// reflinks, hardlinks or normal copy operations to
    /// populate based on the best strategy.
    Native,
    /// An `fstree` backed by an EROFS meta-only image and
    /// overlay mount to provide deduplicated content and
    /// per file metadata.
    Overlayimg,
}

impl Format {
    pub const ALL: [Self; 2] = [Self::Native, Self::Overlayimg];
}

/// A driver capable of managing the lifecycle of an `fstree` for a specific [`Format`].
pub trait Driver {
    /// Driver specific error
    type Error;

    /// Blit a new `fstree` to `target` from the supplied virtual fstree
    /// and asset backing from [`Installation`].
    fn blit(
        &self,
        installation: &Installation,
        tree: &vfs::Tree<PendingFile>,
        target: &Path,
    ) -> Result<(), Self::Error>;

    /// Bring up an `fstree` at the `target` path with the requested `Mutability`.
    ///
    /// Some types of fstrees require mounting to be active & usable. That happens
    /// at this layer, if needed.
    fn bring_up(
        &self,
        _installation: &Installation,
        _target: &Path,
        _mutability: Mutability,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Bring down an `fstree` at the `target` path.
    ///
    /// Some types of fstrees require unmounting to be disabled. That happens
    /// at this layer, if needed.
    fn bring_down(&self, _target: &Path) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// An error from a driver
#[derive(Debug, Error)]
pub enum DriverError {
    #[error("native fstree driver")]
    Native(#[source] native::Error),
    #[error("overlayimg fstree driver")]
    Overlayimg(#[source] overlayimg::Error),
}

/// A type erased [`Driver`]
#[derive(Clone)]
pub struct AnyDriver {
    inner: Arc<dyn Driver<Error = DriverError> + Send + Sync + 'static>,
    /// Format of the `fstree` this driver operates on.
    pub format: Format,
}

impl AnyDriver {
    fn new<T: Driver + Send + Sync + 'static>(inner: T, format: Format, f: fn(T::Error) -> DriverError) -> Self {
        struct Adapter<T: Driver> {
            inner: T,
            f: fn(T::Error) -> DriverError,
        }

        impl<T: Driver> Adapter<T> {
            fn new(inner: T, f: fn(T::Error) -> DriverError) -> Self {
                Self { inner, f }
            }
        }

        impl<T: Driver> Driver for Adapter<T> {
            type Error = DriverError;

            fn blit(
                &self,
                installation: &Installation,
                tree: &vfs::Tree<PendingFile>,
                target: &Path,
            ) -> Result<(), Self::Error> {
                self.inner.blit(installation, tree, target).map_err(self.f)
            }

            fn bring_up(
                &self,
                installation: &Installation,
                target: &Path,
                mutability: Mutability,
            ) -> Result<(), Self::Error> {
                self.inner.bring_up(installation, target, mutability).map_err(self.f)
            }

            fn bring_down(&self, target: &Path) -> Result<(), Self::Error> {
                self.inner.bring_down(target).map_err(self.f)
            }
        }

        Self {
            inner: Arc::new(Adapter::new(inner, f)),
            format,
        }
    }

    /// Create an erased native driver
    pub fn native() -> Self {
        Self::new(NativeDriver, Format::Native, DriverError::Native)
    }

    /// Create an erased overlayimg driver
    pub fn overlayimg() -> Self {
        Self::new(OverlayimgDriver::default(), Format::Overlayimg, DriverError::Overlayimg)
    }

    /// Blit a new `fstree` to `target` from the supplied virtual fstree
    /// and asset backing from [`Installation`].
    pub fn blit<'a>(
        &'a self,
        installation: &'a Installation,
        vfs: &vfs::Tree<PendingFile>,
        target: PathBuf,
    ) -> Result<Fstree<'a>, DriverError> {
        self.inner.blit(installation, vfs, &target)?;

        Ok(Fstree {
            driver: self.clone(),
            installation,
            path: target,
            status: Status::Down,
        })
    }
}

impl Driver for AnyDriver {
    type Error = DriverError;

    fn blit(
        &self,
        installation: &Installation,
        tree: &vfs::Tree<PendingFile>,
        target: &Path,
    ) -> Result<(), Self::Error> {
        self.inner.blit(installation, tree, target)
    }

    fn bring_up(&self, installation: &Installation, target: &Path, mutability: Mutability) -> Result<(), Self::Error> {
        self.inner.bring_up(installation, target, mutability)
    }

    fn bring_down(&self, target: &Path) -> Result<(), Self::Error> {
        self.inner.bring_down(target)
    }
}

/// Handle to an `fstree`.
#[derive(Clone)]
pub struct Fstree<'a> {
    driver: AnyDriver,
    /// The installation providing the CAS backing to this fstree.
    installation: &'a Installation,
    /// Path to this `fstree`.
    pub path: PathBuf,
    /// Stateful status of this `fstree`.
    pub status: Status,
}

impl<'a> Fstree<'a> {
    /// If the supplied path is an identified `fstree`, this returns the `Fstree` handle to that path.
    pub fn identify(installation: &'a Installation, path: PathBuf) -> Option<Fstree<'a>> {
        let format = identify(&path)?;

        let driver = match format {
            Format::Native => AnyDriver::native(),
            Format::Overlayimg => AnyDriver::overlayimg(),
        };

        Some(Fstree {
            driver,
            installation,
            path,
            // TODO: Don't assume this down. Add per driver
            // detection logic for status so we have correct state
            status: Status::Down,
        })
    }

    /// Format of this `fstree`
    pub fn format(&self) -> &Format {
        &self.driver.format
    }

    /// Bring up this `fstree` with the requested [`Mutability`].
    pub fn bring_up(&mut self, mutability: Mutability) -> Result<(), DriverError> {
        self.driver.bring_up(self.installation, &self.path, mutability)?;
        self.status = Status::Up { mutability };
        Ok(())
    }

    /// Bring down this `fstree`.
    pub fn bring_down(&mut self) -> Result<(), DriverError> {
        self.driver.bring_down(&self.path)?;
        self.status = Status::Down;
        Ok(())
    }

    /// Change the mutability of an fstree.
    ///
    /// Returns `true` if the operation was applied.
    ///
    /// Returns `false` if the fstree was already at this mutability or
    /// if the fstree is currently [`Status::Down`].
    pub fn change_mutability(&mut self, new_mutability: Mutability) -> Result<bool, DriverError> {
        match self.status {
            Status::Up { mutability } if mutability != new_mutability => {
                self.driver.bring_down(&self.path)?;
                self.driver.bring_up(self.installation, &self.path, new_mutability)?;
                self.status = Status::Up {
                    mutability: new_mutability,
                };
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Move the `fstree` to a new path
    pub fn move_to(&mut self, to: &Path) -> io::Result<()> {
        // We should only be calling this when the fstree isn't brought up
        debug_assert!(self.status == Status::Down);
        util::ensure_dir_exists(to)?;
        fs::rename(&self.path, to)?;
        self.path = to.to_owned();
        Ok(())
    }
}

/// Stateful status of an fstree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Fstree is down, if applicable.
    Down,
    /// Fstree is up, if applicable.
    ///
    /// See [`Driver::bring_up`].
    Up {
        /// Mutability of the fstree
        mutability: Mutability,
    },
}

/// The requested mutability of an `fstree`
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum Mutability {
    /// Read only
    ReadOnly,
    /// Read write
    ReadWrite,
}

/// A file pending creation to an `fstree`
#[derive(Debug, Clone)]
pub struct PendingFile {
    /// The origin package for this file/inode
    pub id: package::Id,

    /// Corresponding layout entry, describing the inode
    pub layout: StonePayloadLayoutRecord,
}

impl vfs::BlitFile for PendingFile {
    /// Match internal kind to minimalist vfs kind
    fn kind(&self) -> vfs::tree::Kind {
        match &self.layout.file {
            StonePayloadLayoutFile::Symlink(source, _) => vfs::tree::Kind::Symlink(source.clone()),
            StonePayloadLayoutFile::Directory(_) => vfs::tree::Kind::Directory,
            _ => vfs::tree::Kind::Regular,
        }
    }

    /// Return ID for conflict
    fn id(&self) -> AStr {
        self.id.clone().into()
    }

    /// Resolve the target path, including the missing `/usr` prefix
    fn path(&self) -> AStr {
        let result = match &self.layout.file {
            StonePayloadLayoutFile::Regular(_, target) => target.clone(),
            StonePayloadLayoutFile::Symlink(_, target) => target.clone(),
            StonePayloadLayoutFile::Directory(target) => target.clone(),
            StonePayloadLayoutFile::CharacterDevice(target) => target.clone(),
            StonePayloadLayoutFile::BlockDevice(target) => target.clone(),
            StonePayloadLayoutFile::Fifo(target) => target.clone(),
            StonePayloadLayoutFile::Socket(target) => target.clone(),
            StonePayloadLayoutFile::Unknown(.., target) => target.clone(),
        };

        vfs::path::join("/usr", &result)
    }

    /// Clone the node to a reparented path, for symlink resolution
    fn cloned_to(&self, path: AStr) -> Self {
        let mut new = self.clone();
        new.layout.file = match &self.layout.file {
            StonePayloadLayoutFile::Regular(source, _) => StonePayloadLayoutFile::Regular(*source, path),
            StonePayloadLayoutFile::Symlink(source, _) => StonePayloadLayoutFile::Symlink(source.clone(), path),
            StonePayloadLayoutFile::Directory(_) => StonePayloadLayoutFile::Directory(path),
            StonePayloadLayoutFile::CharacterDevice(_) => StonePayloadLayoutFile::CharacterDevice(path),
            StonePayloadLayoutFile::BlockDevice(_) => StonePayloadLayoutFile::BlockDevice(path),
            StonePayloadLayoutFile::Fifo(_) => StonePayloadLayoutFile::Fifo(path),
            StonePayloadLayoutFile::Socket(_) => StonePayloadLayoutFile::Socket(path),
            StonePayloadLayoutFile::Unknown(source, _) => StonePayloadLayoutFile::Unknown(source.clone(), path),
        };
        new
    }
}

impl From<AStr> for PendingFile {
    fn from(value: AStr) -> Self {
        PendingFile {
            id: Default::default(),
            layout: StonePayloadLayoutRecord {
                uid: 0,
                gid: 0,
                mode: 0o755,
                tag: 0,
                file: StonePayloadLayoutFile::Directory(value),
            },
        }
    }
}

impl fmt::Display for PendingFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as vfs::BlitFile>::path(self).fmt(f)
    }
}

impl AsRef<StonePayloadLayoutRecord> for PendingFile {
    fn as_ref(&self) -> &StonePayloadLayoutRecord {
        &self.layout
    }
}

/// Build a [`vfs::Tree`] for the specified layouts.
///
/// Returns a newly built [`vfs::Tree`] that can be used in
/// the creation of fstrees.
pub fn vfs(layouts: Vec<(package::Id, StonePayloadLayoutRecord)>) -> Result<vfs::Tree<PendingFile>, vfs::tree::Error> {
    let mut tbuild = vfs::TreeBuilder::new();

    for (id, layout) in layouts {
        tbuild.push(PendingFile { id: id.clone(), layout });
    }

    tbuild.bake();
    tbuild.tree()
}

/// Identify the format of an `fstree` at the provided path
pub fn identify(path: &Path) -> Option<Format> {
    if overlayimg::is_valid_fstree(path) {
        Some(Format::Overlayimg)
    } else if path.join("usr").exists() {
        Some(Format::Native)
    } else {
        None
    }
}
