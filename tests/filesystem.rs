#![cfg(target_os = "linux")]

use greppy_rift_core::{Backend, probe, remove_snapshot, snapshot_exact};
use std::path::PathBuf;

#[test]
fn native_filesystem_contract() {
    let source = required_path("GREPPY_COW_TEST_SOURCE");
    let destination_root = required_path("GREPPY_COW_TEST_DESTINATION_ROOT");
    let expected = std::env::var("GREPPY_COW_EXPECT_BACKEND")
        .expect("GREPPY_COW_EXPECT_BACKEND must be set by the filesystem harness");
    let original = std::fs::read(source.join("nested/file.txt")).unwrap();

    if expected == "unavailable" {
        assert!(probe(&source, &destination_root).is_err());
        assert_eq!(
            std::fs::read(source.join("nested/file.txt")).unwrap(),
            original
        );
        return;
    }

    let expected = match expected.as_str() {
        "btrfs_snapshot" => Backend::BtrfsSnapshot,
        "linux_reflink_tree" => Backend::LinuxReflinkTree,
        value => panic!("unknown expected backend: {value}"),
    };
    let capability = probe(&source, &destination_root).unwrap();
    assert_eq!(capability.backend, expected);
    assert_eq!(
        capability.constant_time_metadata,
        expected == Backend::BtrfsSnapshot
    );

    let destination = destination_root.join("snapshot");
    let receipt = snapshot_exact(&source, &destination).unwrap();
    assert_eq!(receipt.backend, expected);
    std::fs::write(destination.join("nested/file.txt"), b"snapshot\n").unwrap();
    assert_eq!(
        std::fs::read(source.join("nested/file.txt")).unwrap(),
        original
    );

    assert!(remove_snapshot(&destination).unwrap().removed);
    assert!(!remove_snapshot(&destination).unwrap().removed);
}

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{name} must be set by the filesystem harness"))
}
