use crate::{Error, Result};
use std::path::Path;

const BTRFS_SUPER_MAGIC: libc::c_long = 0x9123_683e;
const BTRFS_IOC_SNAP_CREATE: libc::c_ulong = 0x5000_9401;
const BTRFS_IOC_SUBVOL_CREATE: libc::c_ulong = 0x5000_940e;
const BTRFS_IOC_SNAP_DESTROY: libc::c_ulong = 0x5000_940f;
const BTRFS_IOC_FS_INFO: libc::c_ulong = 0x8400_941f;
const BTRFS_IOC_SUBVOL_GETFLAGS: libc::c_ulong = 0x8008_9419;
const BTRFS_IOC_SUBVOL_SETFLAGS: libc::c_ulong = 0x4008_941a;
const BTRFS_SUBVOL_RDONLY: u64 = 1 << 1;

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

pub(super) fn filesystem_id(path: &Path) -> Result<[u8; 16]> {
    use std::fs::File;
    use std::os::fd::AsRawFd;

    let file = File::open(path)?;
    let mut info = BtrfsIoctlFsInfoArgs::default();
    // SAFETY: file is a live Btrfs path and info is the exact 1024-byte UAPI
    // structure expected by BTRFS_IOC_FS_INFO.
    if unsafe { libc::ioctl(file.as_raw_fd(), BTRFS_IOC_FS_INFO, &mut info) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(info.fsid)
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

pub(super) fn create_subvolume(path: &Path) -> Result<()> {
    path_ioctl(
        path,
        BTRFS_IOC_SUBVOL_CREATE,
        None,
        "create source subvolume",
    )
}

pub(super) fn is_read_only(path: &Path) -> Result<bool> {
    use std::fs::File;
    use std::os::fd::AsRawFd;

    let file = File::open(path)?;
    let mut flags = 0u64;
    // SAFETY: file is a live Btrfs subvolume and flags is the exact u64 UAPI
    // value expected by BTRFS_IOC_SUBVOL_GETFLAGS.
    if unsafe { libc::ioctl(file.as_raw_fd(), BTRFS_IOC_SUBVOL_GETFLAGS, &mut flags) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(flags & BTRFS_SUBVOL_RDONLY != 0)
}

pub(super) fn set_read_only(path: &Path, read_only: bool) -> Result<()> {
    use std::fs::File;
    use std::os::fd::AsRawFd;

    let file = File::open(path)?;
    let mut flags = 0u64;
    // Preserve every flag owned by the kernel rather than replacing the mask.
    if unsafe { libc::ioctl(file.as_raw_fd(), BTRFS_IOC_SUBVOL_GETFLAGS, &mut flags) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if read_only {
        flags |= BTRFS_SUBVOL_RDONLY;
    } else {
        flags &= !BTRFS_SUBVOL_RDONLY;
    }
    // SAFETY: file is a live Btrfs subvolume and flags is the exact u64 UAPI
    // value expected by BTRFS_IOC_SUBVOL_SETFLAGS.
    if unsafe { libc::ioctl(file.as_raw_fd(), BTRFS_IOC_SUBVOL_SETFLAGS, &flags) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub(super) fn remove_snapshot(path: &Path) -> Result<()> {
    match path_ioctl(path, BTRFS_IOC_SNAP_DESTROY, None, "remove snapshot") {
        Ok(()) => Ok(()),
        Err(Error::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
            ) =>
        {
            remove_empty_subvolume(path)
        }
        Err(error) => Err(error),
    }
}

fn remove_empty_subvolume(path: &Path) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    std::fs::remove_dir(path)?;
    Ok(())
}

#[repr(C)]
struct BtrfsIoctlVolArgs {
    fd: i64,
    name: [libc::c_char; 4088],
}

#[repr(C)]
struct BtrfsIoctlFsInfoArgs {
    max_id: u64,
    num_devices: u64,
    fsid: [u8; 16],
    nodesize: u32,
    sectorsize: u32,
    clone_alignment: u32,
    csum_type: u16,
    csum_size: u16,
    flags: u64,
    reserved: [u8; 968],
}

impl Default for BtrfsIoctlFsInfoArgs {
    fn default() -> Self {
        Self {
            max_id: 0,
            num_devices: 0,
            fsid: [0; 16],
            nodesize: 0,
            sectorsize: 0,
            clone_alignment: 0,
            csum_type: 0,
            csum_size: 0,
            flags: 0,
            reserved: [0; 968],
        }
    }
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
    let error = std::io::Error::last_os_error();
    if request == BTRFS_IOC_SNAP_DESTROY {
        return Err(Error::Io(error));
    }
    Err(Error::Unavailable(format!(
        "failed to {action} {}: {}",
        path.display(),
        error
    )))
}

fn c_path(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Error::InvalidPath(format!("path contains a null byte: {}", path.display())))
}
