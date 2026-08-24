use crate::{Error, Result};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use walkdir::WalkDir;

static PROBE_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn verify(destination_root: &Path) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let id = PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let stem = format!(".greppy-cow-probe-{}-{id}", std::process::id());
    let source = destination_root.join(&stem);
    let destination = destination_root.join(format!("{stem}-clone"));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&source)?;
        file.write_all(b"greppy-cow-probe")?;
        reflink_file(&source, &destination)
    })();
    let cleanup_result = [&destination, &source]
        .into_iter()
        .filter(|path| path.exists())
        .try_for_each(fs::remove_file);
    result.and(cleanup_result.map_err(Error::from))
}

pub(super) fn snapshot_exact(from: &Path, to: &Path) -> Result<()> {
    use std::collections::HashMap;
    use std::os::unix::fs::MetadataExt;

    fs::create_dir(to)?;
    let mut hard_links = HashMap::new();
    let mut directories = Vec::new();
    for entry in WalkDir::new(from).min_depth(1).follow_links(false) {
        let entry = entry.map_err(|error| {
            Error::Io(
                error
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("failed to walk snapshot source")),
            )
        })?;
        let source = entry.path();
        let relative = source
            .strip_prefix(from)
            .map_err(|error| Error::InvalidPath(error.to_string()))?;
        let destination = to.join(relative);
        let metadata = fs::symlink_metadata(source)?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            fs::create_dir(&destination)?;
            directories.push((source.to_path_buf(), destination));
        } else if file_type.is_file() {
            let key = (metadata.dev(), metadata.ino());
            if metadata.nlink() > 1 {
                if let Some(existing) = hard_links.get(&key) {
                    fs::hard_link(existing, &destination)?;
                } else {
                    reflink_file(source, &destination)?;
                    hard_links.insert(key, destination.clone());
                }
            } else {
                reflink_file(source, &destination)?;
            }
            copy_metadata(source, &destination, MetadataTarget::FileOrDirectory)?;
        } else if file_type.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(source)?, &destination)?;
            copy_metadata(source, &destination, MetadataTarget::Symlink)?;
        } else {
            return Err(Error::UnsupportedEntry(source.to_path_buf()));
        }
    }
    for (source, destination) in directories.into_iter().rev() {
        copy_metadata(&source, &destination, MetadataTarget::FileOrDirectory)?;
    }
    copy_metadata(from, to, MetadataTarget::FileOrDirectory)
}

fn reflink_file(from: &Path, to: &Path) -> Result<()> {
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;

    const FICLONE: libc::c_ulong = 0x4004_9409;
    let source = File::open(from)?;
    let destination = OpenOptions::new().write(true).create_new(true).open(to)?;
    // SAFETY: both file descriptors are live; FICLONE reads the source and
    // initializes the destination's data extents.
    if unsafe { libc::ioctl(destination.as_raw_fd(), FICLONE, source.as_raw_fd()) } == 0 {
        return Ok(());
    }
    Err(Error::Unavailable(format!(
        "failed to reflink {}: {}",
        from.display(),
        std::io::Error::last_os_error()
    )))
}

#[derive(Clone, Copy)]
enum MetadataTarget {
    FileOrDirectory,
    Symlink,
}

fn copy_metadata(from: &Path, to: &Path, target: MetadataTarget) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata = fs::symlink_metadata(from)?;
    let destination = c_path(to)?;
    // SAFETY: destination is a live path; uid/gid originate from source metadata.
    if unsafe { libc::lchown(destination.as_ptr(), metadata.uid(), metadata.gid()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if matches!(target, MetadataTarget::FileOrDirectory) {
        fs::set_permissions(to, fs::Permissions::from_mode(metadata.mode()))?;
    }
    copy_xattrs(from, to)?;
    let times = [
        libc::timespec {
            tv_sec: metadata.atime(),
            tv_nsec: metadata.atime_nsec(),
        },
        libc::timespec {
            tv_sec: metadata.mtime(),
            tv_nsec: metadata.mtime_nsec(),
        },
    ];
    // SAFETY: destination and times remain live for the duration of the call.
    if unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            destination.as_ptr(),
            times.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn copy_xattrs(from: &Path, to: &Path) -> Result<()> {
    let from = c_path(from)?;
    let to = c_path(to)?;
    // SAFETY: null value asks the kernel for the required list size.
    let size = unsafe { libc::llistxattr(from.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut names = vec![0_u8; size as usize];
    if size > 0
        // SAFETY: names has exactly the capacity reported above.
        && unsafe { libc::llistxattr(from.as_ptr(), names.as_mut_ptr().cast(), names.len()) } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    for name in names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name = std::ffi::CString::new(name)
            .map_err(|_| Error::InvalidPath("xattr name contains a null byte".into()))?;
        // SAFETY: null value asks the kernel for this attribute's size.
        let size =
            unsafe { libc::lgetxattr(from.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
        if size < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut value = vec![0_u8; size as usize];
        if size > 0
            // SAFETY: value has exactly the capacity reported above.
            && unsafe {
                libc::lgetxattr(
                    from.as_ptr(),
                    name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                )
            } < 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: all pointers remain valid for the duration of the call.
        if unsafe {
            libc::lsetxattr(
                to.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}

fn c_path(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Error::InvalidPath(format!("path contains a null byte: {}", path.display())))
}
