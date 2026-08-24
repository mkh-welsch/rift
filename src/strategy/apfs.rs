use crate::{Backend, Capability, Error, Result};
use std::path::Path;

pub(super) fn probe(source: &Path, destination_root: &Path) -> Result<Capability> {
    use std::os::unix::fs::MetadataExt;

    if std::fs::metadata(source)?.dev() != std::fs::metadata(destination_root)?.dev() {
        return Err(Error::Unavailable(format!(
            "APFS clonefile requires source and destination on the same volume: {}",
            destination_root.display()
        )));
    }
    if filesystem_name(source)? != "apfs" {
        return Err(Error::Unavailable(format!(
            "{} is not on APFS",
            source.display()
        )));
    }
    Ok(Capability {
        backend: Backend::ApfsClonefile,
        constant_time_metadata: true,
    })
}

pub(super) fn snapshot_exact(from: &Path, to: &Path) -> Result<()> {
    let source = c_path(from)?;
    let destination = c_path(to)?;
    // SAFETY: both values are live, null-terminated filesystem paths.
    if unsafe { libc::clonefile(source.as_ptr(), destination.as_ptr(), 0) } == 0 {
        return Ok(());
    }
    Err(Error::Unavailable(format!(
        "failed to clone {} to {}: {}",
        from.display(),
        to.display(),
        std::io::Error::last_os_error()
    )))
}

fn filesystem_name(path: &Path) -> Result<String> {
    let path = c_path(path)?;
    // SAFETY: statfs is a plain C structure filled by the kernel.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: path is a live C string and stat points to writable memory.
    if unsafe { libc::statfs(path.as_ptr(), &mut stat) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: macOS guarantees a null-terminated f_fstypename array.
    let name = unsafe { std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr()) };
    Ok(name.to_string_lossy().into_owned())
}

fn c_path(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Error::InvalidPath(format!("path contains a null byte: {}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn exact_snapshot_is_private_and_removable() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/file.txt"), "source").unwrap();

        let capability = probe(&source, temp.path()).unwrap();
        assert_eq!(capability.backend, Backend::ApfsClonefile);
        assert!(capability.constant_time_metadata);

        let receipt = crate::snapshot_exact(&source, &destination).unwrap();
        assert_eq!(receipt.backend, Backend::ApfsClonefile);
        std::fs::write(destination.join("nested/file.txt"), "snapshot").unwrap();
        assert_eq!(
            std::fs::read_to_string(source.join("nested/file.txt")).unwrap(),
            "source"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("nested/file.txt")).unwrap(),
            "snapshot"
        );

        assert!(crate::remove_snapshot(&destination).unwrap().removed);
        assert!(!crate::remove_snapshot(&destination).unwrap().removed);
    }
}
