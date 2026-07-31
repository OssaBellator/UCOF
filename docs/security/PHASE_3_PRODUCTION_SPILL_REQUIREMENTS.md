# Phase 3 Production Spill and Publication Requirements

**Status:** Draft requirements for review  
**Scope:** Large deterministic writers, repair, rewrite, and compaction

## Purpose

Large UCOF operations may exceed caller memory budgets and require temporary runs, staged pages, or rewritten object records. Existing experiments demonstrate bounded sorting, descriptor-limited merging, private staging, ownership-token cleanup, no-follow opens, no-overwrite publication, and synchronization order. This document defines the additional requirements that must be met before those experiments can be described as production spill handling.

These requirements are implementation policy, not wire-format conformance, unless a later specification explicitly promotes one.

## Threats

A production spill subsystem must consider:

- plaintext disclosure through temporary files, swap, crash dumps, backups, snapshots, indexing, or antivirus tools;
- symlink, hard-link, mount-point, path-replacement, and namespace races;
- reuse or deletion of a path not owned by the current operation;
- byte, inode, descriptor, memory, and directory-entry exhaustion;
- partial writes, short writes, delayed allocation, quota failure, and out-of-space conditions;
- process crash or host power loss at every publication step;
- hostile or surprising filesystem durability semantics;
- cross-device publication and non-atomic rename behavior;
- stale staging artifacts after cancellation, panic, or restart;
- rollback, substitution, or mutation of source objects during a remote-backed operation;
- accidental overwrite of an existing destination;
- logs, errors, and metrics leaking filenames, identifiers, payload bytes, keys, or tokens.

## Directory and creation requirements

A production implementation must:

1. accept an explicit caller-selected staging root or use an operating-system private temporary directory;
2. reject a staging root that cannot provide owner-only access semantics appropriate to the platform;
3. create a fresh operation directory with unpredictable name and exclusive creation;
4. open every child relative to an already-open directory handle where the platform permits;
5. use no-follow and exclusive-create behavior and verify the opened object type after opening;
6. refuse symlinks, directories, devices, sockets, and unexpected hard-link counts;
7. attach an unguessable ownership token to the operation and verify it before cleanup;
8. never recursively delete a path solely because its name resembles a staging path;
9. avoid using the destination directory as a general scratch area unless its policy explicitly permits plaintext spill.

## Resource budgets

Before and during operation, independently enforce:

- maximum staged bytes;
- maximum staged files/inodes;
- maximum simultaneously open descriptors;
- maximum sort runs and merge passes;
- maximum record, page, object, and metadata bytes;
- maximum memory retained per run and per merge input;
- maximum elapsed time where deadlines are supported;
- maximum cleanup work and diagnostic count.

Budget checks must use checked arithmetic. Failure to reserve space is not proof that later writes will succeed, so every write and synchronization failure remains terminal.

## Confidentiality policy

The caller must select one of these policies before bytes are staged:

### Plaintext permitted

Allowed only when the caller has determined that the staging filesystem, host, backup, snapshot, indexing, and access-control environment may contain the source plaintext. Documentation must state that deletion cannot guarantee removal from copy-on-write filesystems, journal, swap, snapshots, SSD remapping, or backups.

### Encrypted spill required

- Generate a fresh random data-encryption key per operation.
- Use an authenticated-encryption construction with unique nonce per staged object or framed segment.
- Bind operation token, logical run identifier, segment number, and declared plaintext length as associated data.
- Never reuse nonces with the same key, including after retry or crash recovery.
- Keep keys out of filenames, command lines, environment variables, logs, and persistent metadata.
- Zeroize key material on best effort, without claiming physical secure deletion.
- Treat authentication failure as terminal corruption and never salvage unauthenticated plaintext.
- Define whether crash restart is impossible, requires an externally protected wrapped key, or restarts clean from the source.

The implementation must not claim confidentiality merely because files are private or later unlinked.

## Deterministic content and nondeterministic protection

Encryption may use random nonces and therefore produce nondeterministic staged ciphertext. Canonical UCOF output must remain deterministic after authenticated decryption and canonical merge. Staging identities and encryption metadata must never enter canonical UCOF bytes unless separately specified.

## Write and verification requirements

- Handle short writes and interrupted system calls explicitly.
- Maintain length and digest accounting for every staged segment.
- Verify segment authentication and expected plaintext length when reopening.
- Reject extra bytes, truncated frames, duplicate segment identities, and out-of-order merge input.
- Never trust a staged index without cross-checking the staged record it locates.
- Keep source validation, staged integrity, canonical output construction, and destination publication as separate assurance states.

## Publication state machine

The implementation must expose at least these outcomes:

1. **Not published:** no destination name refers to the new file.
2. **Published and durable:** destination entry and required directory metadata are synchronized according to the documented platform contract.
3. **Publication indeterminate:** a destination entry may exist, but the implementation cannot prove durable success after an error or crash boundary.

It must not report ordinary failure when publication may already have occurred.

### Same-filesystem publication

A preferred no-overwrite sequence is:

1. create and write a private staged output;
2. validate the complete staged UCOF bytes;
3. synchronize staged file data and metadata;
4. atomically link or rename to a destination name using a primitive that refuses overwrite;
5. synchronize the destination directory;
6. retire the private staged name;
7. synchronize the staging directory when required by the platform contract.

The implementation must document which steps are meaningful and guaranteed on each supported platform and filesystem class.

### Cross-filesystem destination

Cross-device rename is not atomic. The implementation must either:

- refuse publication;
- copy into a private file on the destination filesystem, validate and synchronize it there, then use same-filesystem no-overwrite publication; or
- expose a weaker, explicitly named non-atomic publication mode.

Silent fallback to an overwrite-capable copy is prohibited.

## Cleanup and restart

- Cleanup is best effort and must not change a successful publication into a false claim that publication failed.
- Cleanup must verify operation ownership before every destructive action.
- Cancellation and ordinary errors clean only uncommitted artifacts owned by the operation.
- Startup cleanup must use bounded age, count, byte, and directory-depth policies.
- A stale operation whose ownership or publication state is ambiguous must be quarantined or reported, not deleted automatically.
- Restart after source-version change begins as a new operation with new token, budgets, staging directory, and encryption key.

## Platform qualification

Each supported platform must publish evidence for:

- exclusive and no-follow creation behavior;
- hard-link and rename no-overwrite semantics;
- file and directory synchronization behavior;
- behavior on power-loss or fault-injection harnesses where practical;
- path encoding and case-folding effects;
- maximum descriptor and path constraints;
- cleanup behavior after forced termination;
- cross-device handling.

A generic claim of “atomic rename” is insufficient.

## Required tests

Before production status, continuously test:

- symlink and path replacement attacks;
- pre-existing destination and concurrent publisher races;
- byte, inode, descriptor, memory, and quota exhaustion;
- short writes and synchronization failure at every state transition;
- cancellation and deadline before, during, and after staging;
- encrypted-spill nonce uniqueness and authentication failure;
- corrupted, truncated, reordered, duplicated, and substituted spill segments;
- crash boundaries before and after destination linking;
- stale cleanup with forged names and ownership tokens;
- destination publication on each supported platform;
- deterministic final bytes across run size, merge fan-in, and encrypted-spill randomness.

## Non-claims

Even a conforming implementation does not guarantee:

- forensic secure deletion;
- confidentiality from a compromised process or kernel;
- durability beyond the documented platform and storage contract;
- rollback resistance of the final destination;
- preservation of byte-scoped signatures through rewrite;
- semantic correctness of caller-supplied compaction dependencies.
