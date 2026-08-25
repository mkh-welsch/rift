//! Native copy-on-write snapshots for Greppy agent workspaces.
//!
//! This crate deliberately owns no registry, hooks, Git policy, naming, or
//! workspace lifecycle. Greppy remains responsible for those concerns.

mod strategy;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("copy-on-write snapshots are unavailable: {0}")]
    Unavailable(String),
    #[error("path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("snapshot destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("snapshot destination is inside its source: {0}")]
    DestinationInsideSource(PathBuf),
    #[error("unsupported filesystem entry: {0}")]
    UnsupportedEntry(PathBuf),
    #[error(
        "snapshot failed: {snapshot}; cleanup of the partial destination also failed: {cleanup}"
    )]
    CleanupAfterFailure {
        snapshot: Box<Error>,
        cleanup: Box<Error>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    ApfsClonefile,
    BtrfsSnapshot,
    LinuxReflinkTree,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Capability {
    pub backend: Backend,
    /// True only when snapshot creation does not walk every source entry.
    pub constant_time_metadata: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotReceipt {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub backend: Backend,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemoveReceipt {
    pub destination: PathBuf,
    pub removed: bool,
}

/// Creates an empty directory suitable for later use as a snapshot source.
/// On Btrfs this is a subvolume; on other filesystems it is a plain directory.
/// The path must not exist and its parent must already be a directory.
pub fn prepare_snapshot_source(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    if path_exists(path)? {
        return Err(Error::DestinationExists(path.to_path_buf()));
    }
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = canonical_directory(parent)?;
    let path = parent.join(path.file_name().ok_or_else(|| {
        Error::InvalidPath(format!("path has no final component: {}", path.display()))
    })?);
    strategy::prepare_snapshot_source(&path)?;
    Ok(path)
}

/// Reports the exact native CoW backend available for this source and
/// destination root. The probe never modifies the source.
pub fn probe(source: impl AsRef<Path>, destination_root: impl AsRef<Path>) -> Result<Capability> {
    let (source, destination_root) = validated_roots(source.as_ref(), destination_root.as_ref())?;
    strategy::probe(&source, &destination_root)
}

/// Creates an exact CoW snapshot. The destination must not exist.
pub fn snapshot_exact(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<SnapshotReceipt> {
    let source = canonical_directory(source.as_ref())?;
    let destination = destination.as_ref();
    if path_exists(destination)? {
        return Err(Error::DestinationExists(destination.to_path_buf()));
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = canonical_directory(parent)?;
    let destination = parent.join(destination.file_name().ok_or_else(|| {
        Error::InvalidPath(format!(
            "destination has no final component: {}",
            destination.display()
        ))
    })?);
    if destination.starts_with(&source) {
        return Err(Error::DestinationInsideSource(destination));
    }

    let capability = strategy::probe(&source, &parent)?;
    if let Err(error) = strategy::snapshot_exact(&source, &destination, capability.backend) {
        if path_exists(&destination)?
            && let Err(cleanup) = strategy::remove_snapshot(&destination)
        {
            return Err(Error::CleanupAfterFailure {
                snapshot: Box::new(error),
                cleanup: Box::new(cleanup),
            });
        }
        return Err(error);
    }
    Ok(SnapshotReceipt {
        source,
        destination,
        backend: capability.backend,
    })
}

/// Removes a snapshot. Repeated removal is successful and reports
/// `removed = false` after the first call.
pub fn remove_snapshot(destination: impl AsRef<Path>) -> Result<RemoveReceipt> {
    let destination = destination.as_ref().to_path_buf();
    if !path_exists(&destination)? {
        return Ok(RemoveReceipt {
            destination,
            removed: false,
        });
    }
    strategy::remove_snapshot(&destination)?;
    Ok(RemoveReceipt {
        destination,
        removed: true,
    })
}

fn validated_roots(source: &Path, destination_root: &Path) -> Result<(PathBuf, PathBuf)> {
    Ok((
        canonical_directory(source)?,
        canonical_directory(destination_root)?,
    ))
}

fn canonical_directory(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    if !fs::metadata(&path)?.is_dir() {
        return Err(Error::NotDirectory(path));
    }
    Ok(path)
}

fn path_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn destination_must_not_exist() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();

        let error = snapshot_exact(&source, &destination).unwrap_err();
        assert!(matches!(error, Error::DestinationExists(path) if path == destination));
    }

    #[cfg(unix)]
    #[test]
    fn dangling_destination_symlink_counts_as_existing() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir(&source).unwrap();
        std::os::unix::fs::symlink(temp.path().join("missing"), &destination).unwrap();

        let error = snapshot_exact(&source, &destination).unwrap_err();
        assert!(matches!(error, Error::DestinationExists(path) if path == destination));
    }

    #[test]
    fn destination_must_not_be_inside_source() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();

        let error = snapshot_exact(&source, source.join("snapshot")).unwrap_err();
        assert!(matches!(error, Error::DestinationInsideSource(_)));
    }

    #[test]
    fn removal_is_idempotent_for_missing_paths() {
        let temp = TempDir::new().unwrap();
        let destination = temp.path().join("missing");

        let receipt = remove_snapshot(&destination).unwrap();
        assert!(!receipt.removed);
        assert_eq!(receipt.destination, destination);
    }

    #[test]
    fn prepared_snapshot_source_is_empty_and_existing_paths_are_refused() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");

        let prepared = prepare_snapshot_source(&source).unwrap();
        assert!(source.is_dir());
        assert_eq!(prepared, fs::canonicalize(&source).unwrap());
        assert_eq!(fs::read_dir(&source).unwrap().count(), 0);
        assert!(matches!(
            prepare_snapshot_source(&source).unwrap_err(),
            Error::DestinationExists(path) if path == source
        ));
    }
}
