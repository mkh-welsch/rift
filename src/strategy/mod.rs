#[cfg(not(target_os = "linux"))]
use crate::Error;
use crate::{Backend, Capability, Result};
use std::path::Path;

#[cfg(target_os = "macos")]
mod apfs;
#[cfg(target_os = "linux")]
mod btrfs;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod reflink;

pub(super) fn probe(source: &Path, destination_root: &Path) -> Result<Capability> {
    #[cfg(target_os = "macos")]
    return apfs::probe(source, destination_root);

    #[cfg(target_os = "linux")]
    return linux::probe(source, destination_root);

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err(Error::Unavailable(
        "only macOS and Linux are currently supported".into(),
    ))
}

pub(super) fn snapshot_exact(from: &Path, to: &Path, backend: Backend) -> Result<()> {
    #[cfg(target_os = "macos")]
    return match backend {
        Backend::ApfsClonefile => apfs::snapshot_exact(from, to),
        _ => Err(Error::Unavailable(format!(
            "backend {backend:?} is not available on macOS"
        ))),
    };

    #[cfg(target_os = "linux")]
    return linux::snapshot_exact(from, to, backend);

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err(Error::Unavailable(format!(
        "backend {backend:?} is not available on this platform"
    )))
}

pub(super) fn remove_snapshot(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::fs::remove_dir_all(path)?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    return linux::remove_snapshot(path);

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err(Error::Unavailable(
        "snapshot removal is not implemented on this platform".into(),
    ))
}
