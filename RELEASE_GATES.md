# Greppy 0.3.3 Filesystem-CoW release gates

Filesystem-CoW is the sole feature planned for Greppy 0.3.3. Greppy 0.3.2 is
the immutable behavior and release fallback.

The 0.3.3 release is a GO only when all of these conditions hold:

1. Capability detection is fail-closed and never mutates the source.
2. Exact APFS, Btrfs and Linux-FICLONE backends pass their native filesystem
   tests; capability metadata truthfully distinguishes Btrfs constant-time
   snapshots from APFS/FICLONE tree traversal, and unsupported or non-constant
   automatic backends select Greppy's native-worktree fallback.
3. The source tree, index and refs are byte-for-byte unchanged after snapshot,
   agent execution, proposal creation, failure injection and cleanup.
4. The final proposal tree and commit semantics match the 0.3.2 native
   Git-worktree path for the same task.
5. Ten concurrent agents meet the platform-specific startup budget without
   shared writable state or destination collisions.
6. Interrupted creation and repeated cleanup leave no registered live
   workspace and no partial destination.
7. Store-CoW, quotas, sandboxing and lifecycle journaling remain owned and
   enforced by Greppy rather than this crate.
8. Package licenses, provenance, source archives, checksums and release
   attestations include this derivative correctly.

If any mandatory gate fails, no partial CoW implementation or compatibility
layer is released. The 0.3.3 candidate is abandoned, behavior returns to the
published Greppy 0.3.2 baseline, and a different 0.3.3 feature is selected in
a separate decision.
