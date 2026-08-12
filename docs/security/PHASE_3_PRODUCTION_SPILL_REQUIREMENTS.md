# Phase 3 Production Spill and Publication Requirements

**Status:** Draft implementation requirements for review  
**Rebased:** 2026-08-13 against consolidated `main`  
**Scope:** large deterministic writers, repair, rewrite, and compaction  
**Tracking:** issues #11 and #76

## Purpose

Large UCOF operations may exceed caller memory budgets and require temporary runs, staged pages, or rewritten object records. The consolidated research implementation already demonstrates bounded sorting, descriptor-limited merging, private staging, ownership-token cleanup, no-overwrite publication models, Unix research publication, directory-identity pinning, and restart/fault evidence.

This document defines the additional requirements that must be met before those experiments can be described as a **production-candidate spill and publication subsystem**.

These are implementation/storage requirements, not EXP-0003 wire-format conformance, unless a later specification explicitly promotes one. EXP-0003 should define byte validity and publication semantics only where the serialized format itself depends on them; it should not normatively prescribe one operating-system filesystem algorithm.

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
- logs, errors, and metrics leaking filenames, identifiers, payload bytes, keys, credentials, or source-version tokens.

## Directory and creation requirements

A production implementation must:

1. accept an explicit caller-selected staging root or use an operating-system private temporary directory;
2. reject a staging root that cannot provide owner-only access semantics appropriate to the platform;
3. create a fresh operation directory with an unpredictable name and exclusive creation;
4. open children relative to already-open directory handles where the platform permits;
5. use no-follow and exclusive-create behavior and verify object type after opening;
6. refuse symlinks, directories, devices, sockets, and unexpected hard-link counts;
7. bind the operation to an unguessable ownership token and verify it before cleanup;
8. never recursively delete a path solely because its name resembles a staging path;
9. pin or otherwise verify directory identity across security-sensitive publication steps where path replacement could change authority;
10. avoid using the destination directory as general scratch space unless its policy explicitly permits plaintext spill.

Descriptor-relative resolution is preferred on platforms that support it. Path-based research behavior on `main` is evidence only and is not production qualification.

## Resource budgets

Before and during operation, independently enforce:

- maximum staged bytes;
- maximum staged files/inodes;
- maximum simultaneously open descriptors;
- maximum sort runs and merge passes;
- maximum record, page, object, and metadata bytes;
- maximum memory retained per run and merge input;
- maximum read/write operations where useful;
- maximum elapsed time where deadlines are supported;
- maximum cleanup work;
- maximum diagnostics retained.

Budget checks must use checked arithmetic. Successful preflight or space reservation does not prove later writes or synchronization will succeed, so every write and synchronization failure remains semantically significant.

## Confidentiality policy

The caller must select one of these policies before source-derived bytes are staged.

### Plaintext permitted

Allowed only when the caller has determined that the staging filesystem, host, backup, snapshot, indexing, and access-control environment may contain source plaintext.

Documentation must state that unlinking or overwriting cannot guarantee removal from copy-on-write filesystems, journals, swap, snapshots, SSD remapping, backups, or forensic storage.

### Encrypted spill required

- Generate a fresh random data-encryption key per operation.
- Use an authenticated-encryption construction with a unique nonce per staged object or framed segment.
- Bind the operation token, logical run identity, segment number, algorithm/version, and declared plaintext length as associated data.
- Never reuse a nonce with the same key, including after retry or crash recovery.
- Keep keys out of filenames, command lines, environment variables, logs, metrics, and ordinary persistent metadata.
- Zeroize key material on a best-effort basis without claiming physical secure deletion.
- Treat authentication failure as terminal corruption and never salvage unauthenticated plaintext.
- Define whether crash restart is impossible, requires an externally protected wrapped key, or restarts clean from the source.
- Define key lifetime and ownership independently from final UCOF encryption features; encrypted spill protects implementation temporaries, not necessarily the final UCOF file.

The implementation must not claim confidentiality merely because temporary files are private or later unlinked.

## Deterministic content and nondeterministic protection

Encrypted spill may use random keys/nonces and therefore produce nondeterministic staged ciphertext.

Canonical UCOF output must remain deterministic wherever the selected UCOF mode promises deterministic bytes. Staging identities, encryption metadata, temporary filenames, run segmentation, merge fan-in, and random protection values must not enter canonical UCOF bytes unless separately specified by the format.

Equivalent logical input processed under different permitted run sizes, merge fan-ins, staging names, or encrypted-spill randomness must reproduce the same canonical final bytes where canonical output is claimed.

## Staged-segment integrity

- Handle short writes and interrupted system calls explicitly.
- Maintain expected length and cryptographic integrity/authentication accounting for every staged segment.
- Verify authentication and expected plaintext length when reopening encrypted segments.
- Reject extra bytes, truncated frames, duplicate segment identities, out-of-order merge input, and substitution.
- Never trust a staged index without cross-checking the staged record or authenticated segment it locates.
- Bind staged metadata strongly enough that reordering, replay, or cross-operation substitution fails closed.
- Keep source validation, staged integrity, canonical output construction, and destination publication as separate assurance states.

## Publication state machine

The implementation must expose at least these outcomes:

1. **Not published:** no destination name is known to refer to the new file.
2. **Published and durable:** the destination entry and required directory metadata are synchronized according to the documented platform/filesystem contract.
3. **Publication indeterminate:** a destination entry may exist, but the implementation cannot prove durable success after an error, crash, or ambiguous link/rename boundary.

It must not report ordinary failure when publication may already have occurred.

A successful UCOF byte validation is not itself proof that publication is durable.

### Same-filesystem publication

A preferred no-overwrite sequence is:

1. create and write a private staged output;
2. validate the complete staged UCOF bytes;
3. synchronize staged file data and required metadata;
4. publish to the destination using a same-filesystem primitive that refuses overwrite;
5. synchronize the destination directory according to the platform contract;
6. record or advance authenticated restart/journal state as required;
7. retire the private staged name;
8. synchronize the staging directory where required.

The implementation must document which steps are meaningful and guaranteed for every supported platform/filesystem class.

### Cross-filesystem destination

Cross-device rename is not atomic. The implementation must either:

- refuse publication;
- copy into a private file on the destination filesystem, validate and synchronize it there, then use same-filesystem no-overwrite publication; or
- expose a weaker, explicitly named non-atomic publication mode with distinct guarantees.

Silent fallback to overwrite-capable copy is prohibited.

## Restart and authenticated journal requirements

Fresh-process restart must not infer durable publication merely from the existence of a destination name.

A production-candidate restart mechanism should provide authenticated, ownership-bound evidence for the operation and publication phase sufficient to classify surviving state.

At minimum:

- journal/operation records bind operation token and expected artifact identities;
- restart does not trust unauthenticated or foreign operation metadata;
- durable-success classification requires evidence that the relevant directory durability boundary completed;
- a valid owned private artifact may be retained for an explicitly safe retry;
- ambiguous ownership or contradictory state is quarantined/reported rather than deleted automatically;
- reappearing supposedly retired names, invalid durable destinations, or owner mismatches are treated as contradictions requiring intervention;
- journal replacement itself must have a documented atomicity/durability contract.

The current fresh-process classification research is logical/filesystem-state evidence, not physical power-loss qualification.

## Cleanup

- Cleanup is best effort and must not downgrade a proven durable publication into a false failure result.
- Cleanup must verify operation ownership before every destructive action.
- Cancellation and ordinary errors clean only uncommitted artifacts owned by the operation.
- Startup cleanup uses bounded age, count, byte, descriptor, and directory-depth policies.
- A stale operation whose ownership or publication state is ambiguous is quarantined or reported rather than removed automatically.
- Restart after source-version change begins as a new operation with a new token, budgets, staging identity, and encryption key.
- Cleanup errors are reported separately from publication outcome.

## Platform qualification

Each supported platform/filesystem combination must publish evidence for:

- exclusive and no-follow creation behavior;
- descriptor-relative/path-resolution guarantees where relied upon;
- hard-link and rename no-overwrite semantics;
- file and directory synchronization behavior;
- crash/power-loss or fault-injection behavior where practical;
- path encoding, normalization, and case-folding effects;
- ownership/effective-user assumptions;
- maximum descriptor/path constraints;
- cleanup behavior after forced termination;
- cross-device handling;
- network/filesystem-class restrictions where local durability assumptions do not hold.

A generic claim such as “atomic rename” or “fsync was called” is insufficient.

## Required qualification tests

Before production-candidate status, continuously test:

- symlink, hard-link, mount/path replacement, and directory-identity attacks;
- pre-existing destination and concurrent publisher races;
- byte, inode, descriptor, memory, and quota exhaustion;
- short writes and synchronization failure at every state transition;
- cancellation and deadline before, during, and after staging;
- encrypted-spill nonce uniqueness across retries/restarts;
- encrypted-spill authentication failure;
- corrupted, truncated, reordered, duplicated, and substituted spill segments;
- crash boundaries before/after publication and directory synchronization;
- restart classification against stale, foreign, contradictory, and ambiguous state;
- stale cleanup with forged names and ownership tokens;
- destination publication on each supported platform/filesystem class;
- deterministic final bytes across run size, merge fan-in, staging location, and encrypted-spill randomness;
- source-version change during remote-backed construction.

## Relationship to EXP-0003

EXP-0003 should require that incomplete publication never masquerade as exact-end valid active state, but it should not require every conforming implementation to use this specific filesystem protocol.

The wire specification may define:

- what constitutes a complete exact-end commit;
- how earlier complete commits remain discoverable through explicitly requested history/recovery semantics;
- what serialized identity/publication facts participate in validation.

Filesystem durability, temporary confidentiality, OS handles, encrypted spill, and restart journals remain implementation/storage qualification unless deliberately standardized later.

## Non-claims

Even a production-candidate implementation does not guarantee:

- forensic secure deletion;
- confidentiality from a compromised process/kernel/hypervisor;
- durability beyond the documented platform/filesystem/storage contract;
- durability on unqualified network/distributed filesystems;
- rollback resistance of the final destination;
- preservation of byte-scoped signatures through rewrite;
- semantic correctness of caller/profile-supplied compaction dependencies;
- that encrypted spill makes the final UCOF file encrypted.
