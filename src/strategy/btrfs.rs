use crate::{Error, Result};
use std::path::Path;

const BTRFS_SUPER_MAGIC: libc::c_long = 0x9123_683e;
const BTRFS_IOC_SNAP_CREATE: libc::c_ulong = 0x5000_9401;
const BTRFS_IOC_SNAP_DESTROY: libc::c_ulong = 0x5000_940f;

pub(super) fn is_filesystem(path: &Path) -> Result<bool> {
    let path = c_path(path)?;
    // SAFETY: statfs is a plain C structure filled by the kernel.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: path is a live C string and stat points to writable memory.
    if unsafe { libc::statfs(path.as_ptr(), &mut stat) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(stat.f_type == BTRFS_SUPER_MAGIC)
}

pub(super) fn is_subvolume(path: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;

    Ok(is_filesystem(path)? && std::fs::metadata(path)?.ino() == 256)
}

pub(super) fn snapshot_exact(from: &Path, to: &Path) -> Result<()> {
    use std::fs::File;
    use std::os::fd::AsRawFd;

    if !is_subvolume(from)? {
        return Err(Error::Unavailable(format!(
            "Btrfs snapshots require a pre-existing source subvolume: {}",
            from.display()
        )));
    }
    let source = File::open(from)?;
    path_ioctl(
        to,
        BTRFS_IOC_SNAP_CREATE,
        Some(source.as_raw_fd()),
        "create snapshot",
    )
}

pub(super) fn remove_snapshot(path: &Path) -> Result<()> {
    path_ioctl(path, BTRFS_IOC_SNAP_DESTROY, None, "remove snapshot")
}

#[repr(C)]
struct BtrfsIoctlVolArgs {
    fd: i64,
    name: [libc::c_char; 4088],
}

fn path_ioctl(
    path: &Path,
    request: libc::c_ulong,
    source_fd: Option<libc::c_int>,
    action: &str,
) -> Result<()> {
    use std::fs::File;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidPath(format!("path has no parent: {}", path.display())))?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::InvalidPath(format!("path has no name: {}", path.display())))?
        .as_bytes();
    if name.is_empty() || name.len() >= 4088 || name.contains(&0) {
        return Err(Error::InvalidPath(format!(
            "invalid Btrfs subvolume name: {}",
            path.display()
        )));
    }
    let mut args = BtrfsIoctlVolArgs {
        fd: source_fd.map_or(0, i64::from),
        name: [0; 4088],
    };
    for (slot, byte) in args.name.iter_mut().zip(name) {
        *slot = *byte as libc::c_char;
    }
    let parent = File::open(parent)?;
    // SAFETY: parent is an open directory fd and args matches btrfs ioctl ABI.
    if unsafe { libc::ioctl(parent.as_raw_fd(), request, &args) } == 0 {
        return Ok(());
    }
    Err(Error::Unavailable(format!(
        "failed to {action} {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    )))
}

fn c_path(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Error::InvalidPath(format!("path contains a null byte: {}", path.display())))
}
