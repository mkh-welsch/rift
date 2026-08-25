use super::{btrfs, reflink};
use crate::{Backend, Capability, Error, Result};
use std::path::Path;

pub(super) fn probe(source: &Path, destination_root: &Path) -> Result<Capability> {
    let source_is_btrfs = btrfs::is_filesystem(source)?;
    let destination_is_btrfs = btrfs::is_filesystem(destination_root)?;
    let same_filesystem = if source_is_btrfs || destination_is_btrfs {
        source_is_btrfs
            && destination_is_btrfs
            && btrfs::filesystem_id(source)? == btrfs::filesystem_id(destination_root)?
    } else {
        device_id(source)? == device_id(destination_root)?
    };
    if !same_filesystem {
        return Err(Error::Unavailable(format!(
            "Linux CoW requires source and destination on the same filesystem: {}",
            destination_root.display()
        )));
    }
    if btrfs::is_subvolume(source)? {
        return Ok(Capability {
            backend: Backend::BtrfsSnapshot,
            constant_time_metadata: true,
            source_immutable: btrfs::is_read_only(source)?,
        });
    }
    reflink::verify(destination_root)?;
    Ok(Capability {
        backend: Backend::LinuxReflinkTree,
        constant_time_metadata: false,
        source_immutable: false,
    })
}

pub(super) fn set_snapshot_source_immutable(path: &Path, immutable: bool) -> Result<bool> {
    if !btrfs::is_subvolume(path)? {
        return Ok(false);
    }
    btrfs::set_read_only(path, immutable)?;
    btrfs::is_read_only(path)
}

pub(super) fn prepare_snapshot_source(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidPath(format!("path has no parent: {}", path.display())))?;
    if btrfs::is_filesystem(parent)? {
        btrfs::create_subvolume(path)
    } else {
        std::fs::create_dir(path)?;
        Ok(())
    }
}

pub(super) fn snapshot_exact(from: &Path, to: &Path, backend: Backend) -> Result<()> {
    match backend {
        Backend::BtrfsSnapshot => btrfs::snapshot_exact(from, to),
        Backend::LinuxReflinkTree => reflink::snapshot_exact(from, to),
        Backend::ApfsClonefile => Err(Error::Unavailable(
            "APFS clonefile is unavailable on Linux".into(),
        )),
    }
}

pub(super) fn remove_snapshot(path: &Path) -> Result<()> {
    if btrfs::is_subvolume(path)? {
        return btrfs::remove_snapshot(path);
    }
    std::fs::remove_dir_all(path)?;
    Ok(())
}

fn device_id(path: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;

    Ok(std::fs::metadata(path)?.dev())
}
