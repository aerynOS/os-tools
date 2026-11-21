// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use fs_err::{self as fs, File};
use stone::{StoneDecodedPayload, StoneReadError};
use thiserror::Error;
use tui::{ProgressBar, ProgressStyle};

use crate::{
    Client, Event, Installation,
    client::{self, cache::asset_path},
    event::{
        self,
        pkg_extract::{self, PkgExtract},
    },
    fstree, installation,
    package::{self, MissingMetaFieldError},
    util,
};

/// Extract the contents of stone files into `output_dir`, one directory per package
///
/// Progress is reported to `publisher`
pub fn extract(client: &Client, stones: Vec<&PathBuf>, output_dir: &Path) -> Result<(), Error> {
    let installation = Installation::open(Path::new("."), None)?;

    util::ensure_dir_exists(output_dir)?;

    let output_dir = output_dir.canonicalize()?;

    let pkg_extract_scope = client.events.root_scope(Event::PkgExtract);
    let extract_packages_task = pkg_extract_scope.start_task(
        PkgExtract::ExtractPackages,
        pkg_extract::PackagesInfo {
            count: stones.len() as u64,
        },
    );
    let progress = extract_packages_task.start_progress(Some(stones.len() as u64), event::Unit::Items);

    for path in stones {
        let rdr = File::open(path).map_err(Error::IO)?;
        let mut reader = stone::read(rdr).map_err(Error::Format)?;

        let payloads = reader.payloads()?.collect::<Result<Vec<_>, _>>()?;
        let content = payloads.iter().find_map(StoneDecodedPayload::content);
        let layouts = payloads.iter().find_map(StoneDecodedPayload::layout);
        let meta = payloads
            .iter()
            .find_map(StoneDecodedPayload::meta)
            .ok_or(Error::MissingMeta)?;

        let Some(layouts) = layouts else {
            extract_packages_task.report_detail(pkg_extract::PackagesDetail::Skipped { path: path.to_owned() });
            progress.increment(1);
            continue;
        };

        let pkg = package::Meta::from_stone_payload(&meta.body).map_err(Error::MalformedMeta)?;
        let pkg_id = package::Id::from(pkg.id());
        let extraction_root = output_dir.join(pkg_id.to_string());

        let extract_packages_scope = extract_packages_task.child_scope(PkgExtract::ExtractPackages);
        let extract_package = extract_packages_scope.start_task(
            pkg_extract::ExtractPackage,
            pkg_extract::PackageInfo {
                path: path.to_owned(),
                root: extraction_root.clone(),
            },
        );

        // Cleanup old extraction root
        util::recreate_dir(&extraction_root)?;

        fs::create_dir_all(installation.assets_path("v2"))?;

        let content_dir = installation.cache_path("content");
        let content_path = content_dir.join(pkg_id.to_string());

        fs::create_dir_all(&content_dir)?;

        if let Some(content) = content {
            let mut content_file = File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&content_path)?;

            let _progress = ProgressBar::new(content.header.plain_size).with_style(
                ProgressStyle::with_template("|{bar:20.cyan/bue}| {percent}%")
                    .unwrap()
                    .progress_chars("■≡=- "),
            );
            reader.unpack_content(content, &mut content_file)?;

            // Extract all indices from the `.stoneContent` into hash-indexed unique files
            payloads
                .iter()
                .filter_map(StoneDecodedPayload::index)
                .flat_map(|p| &p.body)
                .map(|idx| {
                    let path = asset_path(&installation, &format!("{:02x}", idx.digest));

                    // This asset already exists
                    if path.exists() {
                        return Ok(());
                    }
                    // Create parent dir
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    // Split file reader over index range
                    let mut file = &content_file;
                    file.seek(SeekFrom::Start(idx.start))?;
                    let mut split_file = (&mut file).take(idx.end - idx.start);

                    let mut output = File::create(&path)?;

                    io::copy(&mut split_file, &mut output)?;

                    Ok(())
                })
                .collect::<Result<Vec<_>, Error>>()?;

            fs::remove_file(&content_path)?;
        }

        let records = layouts
            .body
            .clone()
            .into_iter()
            .map(|layout| (pkg_id.clone(), layout))
            .collect::<Vec<_>>();
        let vfs = fstree::vfs(records).map_err(Error::BuildVfsTree)?;

        fstree::native::blit_root(&installation, &vfs, &extraction_root.canonicalize()?).map_err(Error::BlitFstree)?;

        extract_package.complete(None);
        progress.increment(1);
    }

    // Clean up transient .moss install
    fs::remove_dir_all(installation.root.join(".moss"))?;

    extract_packages_task.complete(None);

    Ok(())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("client")]
    Client(#[from] client::Error),

    #[error("Missing metadata")]
    MissingMeta,

    #[error("malformed meta")]
    MalformedMeta(#[from] MissingMetaFieldError),

    #[error("io")]
    IO(#[from] io::Error),

    #[error("stone format")]
    Format(#[from] StoneReadError),

    #[error("installation")]
    Installation(#[from] installation::Error),

    #[error("build vfs tree")]
    BuildVfsTree(#[source] vfs::tree::Error),

    #[error("blit fstree")]
    BlitFstree(#[source] fstree::native::Error),
}
