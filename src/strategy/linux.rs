use super::{btrfs, reflink};
use crate::{Backend, Capability, Error, Result};
use std::path::Path;

pub(super) fn probe(source: &Path, destination_root: &Path) -> Result<Capability> {
    if filesystem_identity(source)? != filesystem_identity(destination_root)? {
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

fn filesystem_identity(path: &Path) -> Result<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;

    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        Error::InvalidPath(format!("path contains a null byte: {}", path.display()))
    })?;
    // SAFETY: statfs is a plain C structure filled by the kernel.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: path is a live C string and stat points to writable memory.
    if unsafe { libc::statfs(path.as_ptr(), &mut stat) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let fsid = &stat.f_fsid;
    // SAFETY: the bytes are copied immediately from a fully initialized
    // kernel-provided fsid value and never outlive it.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            std::ptr::from_ref(fsid).cast::<u8>(),
            std::mem::size_of_val(fsid),
        )
    };
    Ok(bytes.to_vec())
}
