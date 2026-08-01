# Experiment 0098: Unix persistent staging backend

## Question

Can the version-bound private-publication contract from Experiment 0097 be implemented on Unix with exclusive private files, complete validation, same-filesystem no-overwrite publication, and explicit directory synchronization?

## Construction

`PersistentUnixStagingBackend` implements `PersistentStagingBackend` with:

1. a caller-provided private staging directory and destination path;
2. rejection of symlink or group/world-accessible staging directories;
3. rejection of symlink or non-directory destination parents;
4. same-filesystem verification before staging;
5. exclusive mode-0600 read/write staged-file creation under an ownership-token-derived name;
6. staged regular-file, link-count, owner, mode, length, and SHA-256 validation;
7. staged-file `sync_all` before publication;
8. no-overwrite hard-link publication;
9. destination-parent `sync_all` as the destination durability boundary;
10. private-name retirement followed by staging-directory `sync_all`;
11. prepublication abort that removes and synchronizes private state.

The backend is consumed by `stage_and_publish_versioned_source_with_tail`, so whole-file source identity, strong non-ABA version checks, bounded reads and writes, complete staged hashing, and explicit indeterminate outcomes remain unchanged.

## Evidence

Unix Rust tests cover:

- exact durable publication of `base || tail` with private destination mode;
- existing destination preservation and staged-name cleanup;
- symlink staging-directory rejection before file creation;
- source-version change during copying with private-file cleanup and no destination link.

The inherited Experiment 0097 hostile backend fuzzing continues to exercise publication state transitions, while these tests exercise actual Unix file, hard-link, permission, and directory-sync calls.

## Boundary

This remains a path-based plaintext Unix research harness. It does not use descriptor-relative `openat`/`linkat` resolution, establish effective-user or namespace policy, encrypt staged bytes, implement an authenticated durable journal, qualify physical power-loss behavior, define network-filesystem semantics, or establish support for non-Unix platforms.
