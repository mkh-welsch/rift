use super::{btrfs, reflink};
use crate::{Backend, Capability, Error, Result};
use std::path::Path;

pub(super) fn probe(source: &Path, destination_root: &Path) -> Result<Capability> {
    use std::os::unix::fs::MetadataExt;

    if std::fs::metadata(source)?.dev() != std::fs::metadata(destination_root)?.dev() {
        return Err(Error::Unavailable(format!(
            "Linux CoW requires source and destination on the same filesystem: {}",
            destination_root.display()
        )));
    }
    if btrfs::is_subvolume(source)? {
        return Ok(Capability {
            backend: Backend::BtrfsSnapshot,
            constant_time_metadata: true,
        });
    }
    reflink::verify(destination_root)?;
    Ok(Capability {
        backend: Backend::LinuxReflinkTree,
        constant_time_metadata: false,
    })
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
