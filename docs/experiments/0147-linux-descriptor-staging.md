# Experiment 0147 — Linux descriptor-pinned publication staging

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-13  
**Tracking:** issue #11  
**Depends on:** persistent staged-publication state machine and Experiment 0146

## Purpose

Issue #11 requires hardened same-filesystem private staging and deterministic no-overwrite publication without relying on attacker-replaceable directory pathnames after an operation begins. This experiment adds one Linux-specific backend that pins the staging and destination directories once and performs later child-name operations through those already-open directory descriptors.

The implementation preserves the workspace-wide `unsafe_code = "forbid"` policy. It does not use raw `openat`, `linkat`, or `unlinkat` FFI and does not add a systems dependency.

## Descriptor-pinned path model

`PersistentLinuxDescriptorStagingBackend` opens the staging and destination directories at construction and retains those `File` handles for the operation.

On Linux, later child paths are formed through:

```text
/proc/self/fd/<directory-fd>/<child-name>
```

The kernel therefore resolves the child relative to the already-open directory object rather than re-resolving the original directory pathname. The backend also verifies that the procfd entry still denotes the same opened directory before sensitive operations.

This is deliberately Linux/procfs-specific evidence, not a claim of a final cross-platform publication abstraction.

## Private staging invariants

Before accepting staged state, the backend requires the private file to remain:

- a regular file;
- owned by the effective user;
- single-linked before publication;
- inaccessible to group/other permission bits;
- exactly the expected length;
- exactly the expected SHA-256 content identity.

The staged file is synchronized before publication.

The staging directory must be private and suitable for same-filesystem hard-link publication. A symlink staging-directory path is rejected before private creation.

## No-overwrite publication and durability

Publication uses a hard link from the descriptor-pinned staging directory into the descriptor-pinned destination directory.

The result is classified as:

- `Linked` when the destination link is created;
- `DestinationExists` when no-overwrite publication encounters an existing destination;
- `Indeterminate` only when a failed link operation leaves evidence that the destination may already refer to the staged inode.

After a definite link, the destination directory handle is synchronized to establish namespace durability. Private retirement removes the staging name and synchronizes the staging directory.

The existing staged-publication state machine continues to retain private state when link or parent-sync outcome is indeterminate.

## Directory replacement test

The strongest redirect test begins private staging, then renames both original directory paths and creates replacement directories at those old pathnames before validation/publication.

Publication must still appear only under the originally opened destination directory inode. Nothing may be published into the replacement destination path, and cleanup must operate on the originally pinned staging directory.

This demonstrates that later publication steps are not redirected by pathname replacement after the operation has pinned its directory handles.

## Staged-name replacement test

Immediately before publication and cleanup, the backend reopens the staged name relative to the pinned staging directory and compares device/inode identity with the original open staged file.

A test removes the private name and creates attacker-controlled replacement bytes under the same name. The backend must:

- reject publication with `staged name identity`;
- leave the destination absent;
- reject cleanup rather than delete the replacement file.

This converts an observed same-name replacement into a fail-closed result.

## Residual race boundary

This experiment does **not** claim an atomic link-by-open-handle primitive.

The safe standard-library implementation checks staged-name identity and then performs `hard_link` or `remove_file` through procfd. A same-UID actor able to mutate the already-compromised private staging directory could theoretically replace the name in the interval between those operations.

Eliminating that final class of race would require a platform primitive or safe capability API that atomically operates on the already-open file/directory handles. The workspace's `unsafe_code = "forbid"` policy is not weakened to obtain such a primitive.

Accordingly, this backend materially strengthens the documented path-resolution TOCTOU boundary but is not evidence that every same-UID namespace race is impossible.

## Reproduction

```console
cargo test --locked -p ucof-experiments persistent_linux_descriptor_staging_tests
```

The complete repository Rust test, lint, Rust 1.85.0 MSRV, i686, and powerpc64 gates also remain required.

## Remaining issue #11 work

This experiment covers one hardened Linux publication backend. It does not complete issue #11.

Still required include:

- bounded external spill/sort suitable for production-sized workloads;
- explicit encrypted-at-rest spill/staging policy using caller/external key material rather than profile-derived keys;
- authenticated recovery or deterministic discard/restart of encrypted staged state;
- durable restart journal or self-describing checkpoint state;
- startup stale-state cleanup;
- cancellation/crash testing at spill, append, fsync, publish, and parent-sync boundaries;
- ext4/XFS, NTFS, and APFS durability evidence;
- Windows reparse/junction and macOS filesystem hardening equivalents.

## Governance boundary

This is implementation evidence for the bounded successor writer. It does not select EXP-0003 D1–D7, change FCP status, allocate a wire epoch, or make Linux procfs part of the UCOF format.
