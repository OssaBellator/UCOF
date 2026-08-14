# Experiment 0172 — Linux durable nonce journal integration

**Status:** non-normative Phase 3 production-writer crash-authority evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0157, 0170, and 0171

## Purpose

Experiments 0170 and 0171 use real AES-256-GCM for private descriptor staging, but their `DescriptorEncryptionSession` still inherited Experiment 0157's abstract durability boundary: a caller supplied a boolean saying the nonce lease had already been durably committed.

Experiment 0172 replaces that boolean seam with executable Linux filesystem evidence. A nonce lease is now authorized by a real authenticated append-only journal record, and the same `DescriptorEncryptionSession` consumed by the Experiment 0171 encrypted-spill writer is constructed only after the journal record has crossed both durability boundaries:

`reserve lease -> create authenticated generation -> write exact bytes -> file sync_all -> pinned directory sync_all -> activate lease -> issue AES-GCM nonces`

The experiment deliberately does **not** claim that a self-contained filesystem journal provides anti-rollback freshness. A same-authority actor that can delete a valid latest generation can make an older authenticated prefix appear current unless an external trusted monotonic floor is available.

## Journal shape

The journal is append-only. Every committed nonce lease creates a new generation file instead of overwriting or renaming an existing slot.

The canonical private filename is:

`.ucof-nonce-journal-v1-{generation:020}.bin`

Each generation is exactly 128 bytes:

- 96 bytes of canonical journal fields;
- 32-byte HMAC-SHA256 authentication tag.

The 96-byte authenticated body contains:

- magic `UCOFNJ02`;
- format version;
- counter-exhaustion/presence flag and reserved bytes;
- 16-byte derived AES key identity;
- four-byte nonce prefix;
- 16-byte operation identity;
- journal generation;
- leased first counter;
- leased last counter;
- resulting next-unreserved counter state;
- canonical reserved bytes.

The AES key identity is derived as the first 16 bytes of SHA-256 over the Experiment 0172 key-id domain plus the exact AES key. The journal does not persist the AES key itself.

The journal authentication key is a separate 32-byte HMAC key supplied to this experiment. Production provisioning/storage of that key remains out of scope.

## Filesystem authority

The implementation mirrors the repository's Linux descriptor-pinned private staging discipline rather than relying on an ordinary path after open.

The journal directory is opened with:

- `O_DIRECTORY`;
- `O_NOFOLLOW`;
- `O_CLOEXEC`.

The opened directory must:

- still be a directory;
- be owned by the effective UID;
- grant no group/other permission bits.

Subsequent child operations are resolved through `/proc/self/fd/<directory-fd>/...`, and the procfd directory identity is rechecked against the pinned descriptor before use.

Each journal generation is created with:

- `create_new(true)`;
- mode `0600`;
- `O_NOFOLLOW`;
- `O_CLOEXEC`.

This gives competing writers a fail-closed create-new collision rather than an overwrite primitive. It is still test-only Linux evidence, not a completed multi-process locking protocol.

## Commit ordering

`commit_descriptor_session` performs the following sequence:

1. derive and verify the AES key identity;
2. rescan the authenticated journal and require it to equal the caller's current in-memory durable authority;
3. reserve the next Experiment 0157 lease from that durable state;
4. encode and HMAC the exact generation/range record;
5. create the generation file exclusively;
6. write and flush all 128 bytes;
7. `sync_all` the journal file;
8. revalidate the pinned directory identity;
9. `sync_all` the directory;
10. only then activate the Experiment 0157 lease as durably committed;
11. construct the real `DescriptorEncryptionSession` used by Experiments 0170/0171.

There is no API path in this experiment that returns an issuable session at either pre-sync injection cut.

## Recovery rules

Recovery scans the pinned directory under explicit limits and recognizes only canonical generation filenames.

Every recognized journal generation must:

- be a regular file;
- have the exact 128-byte length;
- authenticate under HMAC-SHA256;
- encode canonical reserved bytes;
- name the same generation encoded in its content;
- bind the expected AES key identity;
- bind the expected nonce prefix.

Recognized records are sorted by generation and must form an exact contiguous sequence beginning at generation 1.

The nonce ranges must also be contiguous:

- generation N must begin at the previous generation's `next_unreserved` value;
- its persisted resulting high-water must equal `lease_last + 1`, or the exhausted `None` state after the final `u64` counter.

A tampered record, generation gap, wrong key, wrong prefix, or discontinuous lease range fails recovery closed.

Default experimental bounds are:

- at most 4096 directory entries inspected;
- at most 4096 recognized journal generations;
- at most 4096 × 128 authenticated journal bytes;
- at most 1,000,000 counters in one lease.

Append-only growth is therefore bounded but not compacted. Journal checkpoint/compaction is a separate follow-on problem.

## Real AES writer integration

The primary integration regression reserves exactly `2 * object_count` counters because Experiment 0171 requires one nonce for each encrypted sorter descriptor and one for each retained encrypted descriptor.

For 17 objects:

- generation 1 durably reserves counters 0 through 33;
- the returned real `DescriptorEncryptionSession` drives the Experiment 0171 encrypted-spill writer;
- the writer produces the exact canonical public UCOF bytes and report;
- the session exhausts its 34-counter lease.

After dropping and reopening the journal:

- recovery returns generation 1 with high-water 34;
- generation 2 reserves the next disjoint 34-counter range beginning at 34;
- a second encrypted-spill execution again produces the exact canonical public bytes/report;
- final recovered high-water is 68.

This is the first experiment in the current sequence where the real AES-GCM writer is fed by nonce authority derived from concrete file and directory durability operations rather than a boolean durability assertion.

## Crash-cut behavior

Two injected pre-activation cuts are executable:

### After write, before file sync

The generation file may be visible to the current process, but no encryption session is returned.

If the candidate survives and authenticates on recovery, the lease is conservatively burned. If a simulated crash loses that never-synced candidate entirely, the old high-water can be recovered and the counters can be reused because no session was ever issued.

A torn/truncated surviving candidate is not treated as absence; exact length/authentication fails closed.

### After file sync, before directory sync

Again, no encryption session is returned.

If the directory entry survives, recovery burns the lease. If a real crash/filesystem loses the unsynchronized directory entry, reuse remains safe for the same reason: the session had not yet become issuable.

The experiment does not emulate physical power loss; it establishes the ordering and recovery interpretation around the Linux sync primitives.

## Tamper and topology evidence

Executable regressions prove:

- changing an authenticated journal byte causes HMAC authentication failure;
- deleting an earlier generation while leaving a later generation causes a generation-gap failure;
- a stale in-memory authority cannot commit after another authority has advanced the on-disk journal;
- a different AES key cannot obtain a session from the journal;
- reopening with a different nonce prefix fails closed.

The append-only create-new naming also prevents a normal commit from overwriting an existing generation.

## Rollback boundary

HMAC establishes authenticity, not freshness.

A regression deliberately commits two valid generations, records an external `TrustedNonceFloor`, then deletes the latest generation.

Without the external floor, the remaining generation-1 prefix is internally valid and the filesystem-only journal cannot prove that generation 2 ever existed.

With the trusted floor, recovery requires both:

- a generation no lower than the floor;
- a nonce high-water no lower than the floor, treating exhausted `None` as the maximum state.

The rolled-back prefix is then rejected.

This distinction is intentional: Experiment 0172 does **not** claim anti-rollback authority unless a freshness source outside the rollbackable journal is supplied.

## What this closes

Experiment 0172 closes the most important cryptographic promotion seam left by Experiments 0170/0171:

- real AES-GCM nonce issuance no longer depends on a caller-provided durability boolean;
- the reserved high-water is authenticated before any session can exist;
- file durability precedes directory durability;
- directory durability precedes lease activation;
- restart reconstructs the global nonce high-water for the same AES key/prefix;
- unused counters in a durable lease are burned across restart;
- multiple operations under one AES key/prefix receive globally disjoint counter ranges;
- tampering, missing generations, stale authorities, wrong keys, and wrong prefixes fail closed;
- the same session object feeds the already-proven canonical Experiment 0171 writer.

## What remains open

Issue #11 remains open. Experiment 0172 does **not** establish:

- a production source/provisioning contract for the AES key or journal HMAC key;
- a platform keystore, TPM, HSM, remote KMS, or other external monotonic freshness anchor;
- self-contained rollback detection against an actor able to delete/replay authenticated journal generations;
- physical power-loss qualification of `sync_all` behavior on supported filesystems/hardware;
- journal compaction/checkpointing after the bounded append-only generation limit;
- a full multi-process coordination protocol beyond rescan + exclusive generation creation;
- recovery of the encrypted spill/retained stages themselves after a mid-operation crash;
- authenticated persistence of expected encrypted private-stage identities/lengths for restart classification;
- encrypted locator or page-reference stages;
- confidentiality of visible sorter object-id keys or spill geometry;
- resolution of the repository's documented same-UID final identity-check -> destructive-operation race;
- production qualification outside native Linux x86_64 for the AWS-LC-backed crypto path.

A further architectural caution remains: journal authority and ephemeral spill files currently share a private directory in the integration regression. A production design should likely give long-lived journal authority a dedicated pinned namespace/directory so ephemeral spill quotas, cleanup policy, and durable journal retention cannot be accidentally conflated.

## Verification

Accepted implementation head before this evidence note:

`d4714e62dd3f2847e9e0242399f3ef8b232d26f2`

Repository Rust workflow run:

`31799373644`

At the point this evidence note was written, the accepted implementation head had completed successfully on:

- locked dependency-graph verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0172 journal/AES integration regressions;
- Rust 1.85.0 MSRV and HTTP-feature MSRV;
- i686 portability checks for the generic writer;
- powerpc64 portability checks for the generic writer.

The broader repository replay continues through the same HTTP/source, policy, documentation, independent parser/model, corpus, and framing checks used by Experiments 0170 and 0171. This note should be amended only if one of those wider checks exposes a regression.

## Next executable slice

The highest-value next slice is **encrypted-operation restart classification**, not another cryptographic primitive.

The journal now durably proves which nonce range became authoritative before encryption. The next experiment should persist the expected identities and lengths of the encrypted private stages for that journal generation and connect them to the existing bounded restart inventory/cleanup state machine.

That would let restart distinguish, without guessing:

- durable lease with no surviving stage -> burn lease and restart work;
- exact authenticated surviving encrypted spill/retained stage -> resume or explicitly discard under journal authority;
- renamed exact stage -> resolve by identity;
- conflicting/truncated/tampered stage -> retain indeterminate/fail closed;
- durable public publication vs still-private encrypted working state.

Only after that restart bridge is executable should locator/page-reference confidentiality or journal compaction outrank crash-consistent encrypted-operation recovery.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
