// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    io,
    path::{Path, PathBuf},
};

use fs_err::{self as fs, File};
use nix::{
    mount::{MntFlags, MsFlags, mount, umount2},
    sys::stat::stat,
};
use snafu::{ResultExt, Snafu, ensure_whatever};

use crate::{Installation, util};

use super::{Driver, Mutability, PendingFile};

pub use erofs::XattrNamespace;

#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayimgDriver {
    erofs_image_writer: erofs::MetaImageWriter,
}

impl OverlayimgDriver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_xattr_namespace(self, xattr_namespace: XattrNamespace) -> Self {
        Self {
            erofs_image_writer: self.erofs_image_writer.with_xattr_namespace(xattr_namespace),
        }
    }
}

impl Driver for OverlayimgDriver {
    type Error = Error;

    fn blit(&self, installation: &Installation, tree: &vfs::Tree<PendingFile>, target: &Path) -> Result<(), Error> {
        self.blit(installation, tree, target)
            .with_whatever_context(|_| format!("blit fstree to {}", target.display()))
    }

    fn bring_up(&self, installation: &Installation, target: &Path, mutability: Mutability) -> Result<(), Error> {
        bring_up(installation, target, mutability)
            .with_whatever_context(|_| format!("bring up {mutability} fstree at {}", target.display()))
    }

    fn bring_down(&self, target: &Path) -> Result<(), Error> {
        bring_down(target).with_whatever_context(|_| format!("bring down fstree at {}", target.display()))
    }
}

impl OverlayimgDriver {
    fn blit(&self, installation: &Installation, tree: &vfs::Tree<PendingFile>, target: &Path) -> Result<(), Error> {
        // Constructs all paths
        let paths = Paths::new(target);

        // If this is an existing fstree that is mounted,
        // we need to bring it down to blit the new image.
        let _ = bring_down(target);

        // Scaffold the new fstree
        self.scaffold(&paths).whatever_context("scaffold new fstree")?;

        // Write an EROFS image to the designated path
        let mut erofs_image = File::create(&paths.erofs_image).whatever_context("create erofs.img file")?;
        self.erofs_image_writer
            .write(tree, &installation.assets_path("v2"), &mut erofs_image)
            .whatever_context("write erofs.img to file")?;

        // That's everything! The real magic happens during `bring_up`
        // when we mount everything.
        Ok(())
    }

    /// Scaffolds a new `fstree`.
    fn scaffold(&self, paths: &Paths) -> Result<(), Error> {
        let scaffold_dirs = || -> io::Result<_> {
            // Recreate the fstree
            util::ensure_dir_exists(&paths.root)?;
            fs::create_dir_all(&paths.erofs)?;
            fs::create_dir_all(&paths.extra)?;
            fs::create_dir_all(&paths.work)?;
            fs::create_dir_all(&paths.merged)?;
            Ok(())
        };

        scaffold_dirs().whatever_context("scaffold dirs")
    }
}

/// Required paths used by an overlayimg fstree
struct Paths {
    /// Root `/` of the fstree
    root: PathBuf,
    /// Path we will write the EROFS image to
    erofs_image: PathBuf,
    /// Where we mount the erofs.img
    erofs: PathBuf,
    /// Overlay folder used as an upper layer when
    /// [`Mutability::ReadWrite`] and used as the
    /// first lower layer when [`Mutability::ReadOnly`]
    ///
    /// This is where things like triggers & other extra
    /// files will live that aren't part of the immutable
    /// EROFS base image.
    extra: PathBuf,
    /// Overlay work dir used when [`Mutability::ReadWrite`]
    work: PathBuf,
    /// Overlay merged mount dir that holds the final fstree
    /// and will be mounted to `usr/`
    merged: PathBuf,
}

impl Paths {
    fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();

        // State
        let var_fstree = root.join("var/lib/moss/fstree");
        let erofs_image = var_fstree.join("erofs.img");
        let extra = var_fstree.join("extra");

        // Runtime mounts
        let run_fstree = root.join("run/moss/fstree");
        let erofs = run_fstree.join("erofs");
        let work = run_fstree.join("work");
        let merged = root.join("usr");

        Self {
            root,
            erofs_image,
            erofs,
            extra,
            work,
            merged,
        }
    }

    fn is_valid_fstree(&self) -> bool {
        self.erofs_image.exists()
            && self.extra.exists()
            && self.erofs.exists()
            && self.work.exists()
            && self.merged.exists()
    }
}

pub fn is_valid_fstree(target: &Path) -> bool {
    Paths::new(target).is_valid_fstree()
}

pub fn bring_up(installation: &Installation, target: &Path, mutability: Mutability) -> Result<(), Error> {
    // Constructs all paths
    let paths = Paths::new(target);

    // Ensure we only try to bring up if the requested
    // fstree is supported by this driver
    ensure_whatever!(
        paths.is_valid_fstree(),
        "{} is not a valid overlayimg fstree",
        target.display()
    );

    // Mount
    mount_all(installation, mutability, &paths).whatever_context("mount the fstree")?;

    Ok(())
}

pub fn bring_down(target: &Path) -> Result<(), Error> {
    // Constructs all paths
    let paths = Paths::new(target);

    // Ensure we only try to bring down if the requested
    // fstree is supported by this driver
    ensure_whatever!(
        paths.is_valid_fstree(),
        "{} is not a valid overlayimg fstree",
        target.display()
    );

    // Unmount
    unmount_all(&paths).whatever_context("unmount the fstree")?;

    Ok(())
}

fn mount_all(installation: &Installation, mutability: Mutability, paths: &Paths) -> Result<(), Error> {
    // Mount EROFS
    mount(
        Some(&paths.erofs_image),
        &paths.erofs,
        Some("erofs"),
        MsFlags::empty(),
        Some(""),
    )
    .whatever_context("mount erofs.img")?;

    let overlay_options = match mutability {
        Mutability::ReadOnly => format!(
            "lowerdir={}:{}/usr::{}",
            paths.extra.display(),
            paths.erofs.display(),
            installation.assets_path("v2").display(),
        ),
        Mutability::ReadWrite => format!(
            "lowerdir={}/usr::{},upperdir={},workdir={}",
            paths.erofs.display(),
            installation.assets_path("v2").display(),
            paths.extra.display(),
            paths.work.display()
        ),
    };

    // Mount overlay
    mount(
        Some("overlay"),
        &paths.merged,
        Some("overlay"),
        MsFlags::empty(),
        Some(overlay_options.as_str()),
    )
    .whatever_context("mount overlay")?;

    Ok(())
}

fn unmount_all(paths: &Paths) -> Result<(), Error> {
    let stat_path = |path: &Path| stat(path).with_whatever_context(|_| format!("stat {}", path.display()));

    // Stat parent vs mounts so we can compare `st_dev`
    // to validate they are mounted prior to attempting
    // to unmount.
    let root_stat = stat_path(&paths.root)?;
    let erofs_stat = stat_path(&paths.erofs)?;
    let overlay_stat = stat_path(&paths.merged)?;

    if root_stat.st_dev != overlay_stat.st_dev {
        // Unmount overlay
        umount2(&paths.merged, MntFlags::MNT_DETACH).whatever_context("unmount overlay")?;
    }
    if root_stat.st_dev != erofs_stat.st_dev {
        // Unmount erofs
        umount2(&paths.erofs, MntFlags::MNT_DETACH).whatever_context("unmount erofs")?;
    }

    Ok(())
}

#[derive(Debug, Snafu)]
#[snafu(whatever, display("{message}"))]
pub struct Error {
    message: String,
    #[snafu(source(from(Box<dyn std::error::Error + Send + Sync + 'static>, Some)))]
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}
