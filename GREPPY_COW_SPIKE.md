# Greppy Filesystem-CoW spike

Status: **preliminary architecture evidence; no Greppy product integration
approved yet**.

This spike evaluates the smallest useful derivative of Rift for Greppy 0.3.3.
It is pinned to upstream commit
[757a22cb247f9b24a849c9d6bd56f49c0ec494f8](https://github.com/anomalyco/rift/commit/757a22cb247f9b24a849c9d6bd56f49c0ec494f8).
The machine-readable macOS result is
[greppy-spike/results-macos-arm64-apfs.json](greppy-spike/results-macos-arm64-apfs.json).

## What was tested

- all upstream workspace tests: 73 passed;
- APFS exact and filtered Rift creation;
- cold native Git-worktree creation;
- assignment of a pre-created warm Git worktree through git worktree move;
- ten concurrent exact snapshots;
- Git cleanliness, detached HEAD, private editing and isolated commit creation;
- Greppy 0.3.2 indexing, symbol lookup, file reading and editing inside the
  exact snapshot;
- macOS protected com.apple.provenance extended attributes;
- linked Git-worktree input.

The medium fixture is a clean Greppy checkout with 619 tracked files and 136
MiB logical size. The large fixture contains 10,001 tracked files and one
128-MiB file. Measurements ran on macOS 26.2, Apple M5, APFS.

## Results

Median workspace start time:

| Fixture | Rift filtered | Rift exact | Git worktree cold | Warm-pool move |
|---|---:|---:|---:|---:|
| Greppy medium | 175.9 ms | **10.4 ms** | 219.6 ms | 23.2 ms |
| 10k-file fixture | 2935.6 ms | **94.9 ms** | 1006.0 ms | 27.5 ms |

Rift exact is about 21 times faster than a cold Git worktree on the medium
fixture and 10.6 times faster on the 10k-file fixture. Filtered creation walks
the complete tree: it is only about 20% faster than a cold worktree on the
medium fixture and about 2.9 times slower on the large fixture.

A warm pool has excellent hand-off latency because it pays checkout cost and
private storage before measurement. It beats Rift exact on the 10k-file
fixture, but requires fixed pre-provisioned capacity and replenishment. The
0.3.3 comparison must therefore report startup latency, replenishment cost and
private physical storage separately; a claim that CoW always beats warm-pool
hand-off latency would be false.

Ten concurrent exact snapshots completed creation in 339 ms on the medium
fixture and 1183 ms on the large fixture. Cleanup was correct but currently
slow (1751 ms and 6579 ms respectively), so physical deletion belongs off the
agent startup/finish critical path.

## Confirmed incompatibilities

1. The public Manager rejects linked Git worktrees because it requires .git
   to be a directory. Greppy currently uses linked worktrees, so the Manager
   cannot be integrated unchanged.
2. Hooks run by default. Greppy must never execute repository hooks implicitly
   while allocating an agent workspace.
3. Filtered APFS creation copies extended attributes entry by entry. Ordinary
   files carrying protected com.apple.provenance attributes fail with
   Permission denied. Exact APFS clonefile succeeds on the same tree.
4. Linux/Btrfs init may convert and replace the source directory with a
   subvolume. Greppy must not mutate an operator checkout as an implicit agent
   side effect.
5. Windows workspace creation is not implemented upstream.
6. The platform Strategy boundary is private and the public Manager tightly
   couples snapshot mechanics to Rift markers, naming, hooks, Git policy and
   its SQLite registry.
7. The repository declares MIT in Cargo metadata and its README but currently
   has no tracked LICENSE/NOTICE file. Redistribution needs an explicit license
   text and attribution before a production derivative ships.

## Proposed targeted fork

Do not integrate the Rift CLI, Manager, markers, hooks, filter policy,
registry, npm/Bun/Node FFI, shell integration or Git-detach behavior.

Extract and maintain one small Rust crate, provisionally named
greppy-rift-core, containing only:

    probe(source, destination_root) -> Capability
    snapshot_exact(source, destination) -> SnapshotReceipt
    remove_snapshot(destination) -> RemoveReceipt

Selected implementations:

- macOS: exact APFS clonefile;
- Linux/Btrfs: writable subvolume snapshot, but only from an already suitable
  source prepared explicitly outside agent startup;
- Linux on reflink-capable filesystems: per-file FICLONE, declared honestly
  as metadata-traversing rather than O(1);
- every unsupported case: Greppy's existing native-worktree fallback.

Greppy remains the owner of repository preflight, workspace identity,
lifecycle journal, quotas, sandbox policy, Store-CoW composition, Git proposal
creation and cleanup scheduling. The snapshot crate must not modify the source,
execute hooks, create Git refs, or maintain a second workspace registry.

This cuts the dependency surface substantially: the exact APFS path needs
little beyond libc; Greppy does not need Rift's rusqlite, git2, toml, ulid,
rand, CLI or FFI layers for snapshot mechanics.

## Preliminary decision

**Conditional GO for a targeted core fork; NO-GO for integrating Rift as a
whole.**

Before product integration, the following evidence is still mandatory:

1. Repeat exact, filtered, cold-worktree, warm-pool and ten-agent tests on
   Linux/Btrfs and Linux/FICLONE.
2. Resolve the missing license-text/attribution packaging.
3. Add snapshot receipts, source immutability checks, crash injection and
   cleanup idempotency tests.
4. Prove identical final proposal trees between native worktree and CoW
   backends.
5. Replace the current 0.3.3 claim “faster than every warm pool” with a
   multi-dimensional gate covering hand-off latency, replenishment, elastic
   concurrency and physical private storage.

No custom FUSE/FSKit/WinFsp filesystem is justified by the macOS evidence.
