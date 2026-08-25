# greppy-rift-core

`greppy-rift-core` is the deliberately small native copy-on-write snapshot
engine for Greppy agent workspaces. It is a hard fork of selected filesystem
mechanics from [anomalyco/rift](https://github.com/anomalyco/rift); it is not a
general-purpose workspace manager or a drop-in Rift replacement.

The crate exposes exactly four operations:

```rust
probe(source, destination_root) -> Capability
prepare_snapshot_source(path) -> PathBuf
snapshot_exact(source, destination) -> SnapshotReceipt
remove_snapshot(destination) -> RemoveReceipt
```

It contains no CLI, FFI, hook execution, Git policy, registry, marker, naming,
filtering, source conversion, or implicit initialization. Greppy owns the
complete workspace lifecycle and falls back to its native Git-worktree backend
when `probe` returns an error.

`prepare_snapshot_source` creates an empty Btrfs subvolume when its parent is
on Btrfs and an empty ordinary directory elsewhere. It exists solely so Greppy
can populate a reusable snapshot template without invoking the `btrfs` CLI or
converting an existing source tree.

## Backends

| Platform | Backend | Metadata behavior |
|---|---|---|
| macOS/APFS | directory `clonefile` | walks directory metadata; file data is CoW |
| Linux/Btrfs subvolume | writable snapshot | constant-time metadata |
| Linux with `FICLONE` | exact per-file reflink tree | walks metadata |

Btrfs sources must already be subvolumes. This crate never converts, renames,
or replaces a source directory. Unsupported platforms and filesystems fail
closed.

`Capability::constant_time_metadata` is deliberately `false` for APFS
directory clones. Apple's `clonefile(2)` recursively clones the hierarchy; it
does not expose a writable, directory-scoped constant-time snapshot. Consumers
may offer this backend explicitly, but must not select it for an automatic
constant-time workspace policy.

## Safety contract

- the source is canonicalized and never modified;
- source and destination must be on the same filesystem;
- the destination must not exist and cannot be inside the source;
- a partial failed destination is removed before returning, and cleanup
  failure is surfaced explicitly rather than hidden;
- removal is idempotent for paths that no longer exist;
- no repository-provided command or hook is ever executed.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

The release contract for Greppy 0.3.3 is documented in
[`RELEASE_GATES.md`](RELEASE_GATES.md).

## Origin and license

The fork is pinned to Rift commit
[`757a22cb247f9b24a849c9d6bd56f49c0ec494f8`](https://github.com/anomalyco/rift/commit/757a22cb247f9b24a849c9d6bd56f49c0ec494f8).
See [`UPSTREAM.md`](UPSTREAM.md) for retained provenance. The code is available
under the MIT license in [`LICENSE`](LICENSE).
