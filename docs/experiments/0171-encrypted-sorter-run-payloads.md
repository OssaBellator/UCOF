# Experiment 0171 — Encrypted bounded-sorter descriptor payloads

**Status:** non-normative Phase 3 production-writer confidentiality evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0157 and 0170

## Purpose

Experiment 0171 closes the plaintext descriptor exposure that remained in the bounded external sort after Experiment 0170.

The generic bounded sorter itself is unchanged. Instead, each 64-byte source descriptor is encrypted before it reaches the sorter. The sorter continues to order by its existing clear 64-bit key while treating the encrypted payload as an opaque fixed-width record. Merge passes copy that same authenticated ciphertext payload without resealing it.

At final sorter output, an adapter authenticates/decrypts the sorted opaque payload and immediately re-encrypts the descriptor into Experiment 0170's retained 92-byte encrypted descriptor stage. No plaintext retained descriptor stage is written.

## Why the sorter is not forked

The accepted design deliberately avoids a second encrypted sorter implementation or a frame-codec rewrite of the generic sorter.

The existing sorter retains sole authority for:

- initial-run sorting;
- merge ordering;
- duplicate-key rejection;
- run fan-in;
- read/write/live-spill byte accounting;
- cleanup of temporary runs;
- final output ordering.

Experiment 0171 changes only the payload presented to that sorter and the final output sink.

This keeps the ordering and duplicate semantics on one implementation path.

## Encrypted sorter record geometry

Before sorting, a 64-byte descriptor is converted to a 100-byte opaque sorter payload:

- 8-byte authenticated logical key copy;
- 12-byte nonce;
- 64-byte AES-256-GCM ciphertext;
- 16-byte authentication tag.

The unchanged sorter prepends its existing 8-byte ordering key, giving a **108-byte on-disk sorter frame**.

The logical key therefore appears in cleartext in two places:

1. the sorter's outer 64-bit ordering key;
2. the payload's inner authenticated 64-bit logical-key copy.

This experiment protects descriptor payload confidentiality, not object-identifier confidentiality. Object IDs/order keys remain visible in spill files.

The authenticated inner copy exists because the current sorter output API writes only payload bytes to its destination. It lets the final adapter recover a cryptographically authenticated logical key and compare it with the decrypted descriptor identity.

## Spill authentication

The sorter payload AAD binds:

- `UCOF-EXP-0171-SPILL\0` domain separation;
- operation identity;
- durable journal generation;
- authenticated logical key;
- leased nonce counter;
- exact 64-byte descriptor width.

The nonce uses the same Experiment 0170 construction:

- four-byte session prefix;
- 64-bit leased counter in big-endian form.

The inner logical key must equal the decrypted descriptor's `object_id`.

## Two disjoint nonce leases

Each writer execution activates two exact-size leases from one durable nonce authority:

1. **spill lease** — one nonce for every logical input descriptor before sorting;
2. **retained-stage lease** — one nonce for every final sorted descriptor written into the 92-byte retained stage.

For `N` descriptors, the durable high-water mark advances by exactly `2N` counters.

Merge passes do not consume new nonces. They copy the already-authenticated encrypted payload byte-for-byte. This avoids nonce consumption depending on run geometry or merge depth and avoids any AES-GCM nonce reuse caused by resealing during merges.

Copying the same ciphertext across merge generations leaks that two spill records are the same persisted encrypted record, but it does not constitute a new encryption under the same nonce.

Both lease capacities are checked for the full object count before any sorter/private-stage file is created.

## Final authenticated ordering

The sorter's outer clear key is needed for merge ordering but is not trusted as the final logical identity.

The final output adapter:

1. authenticates/decrypts the 100-byte sorter payload;
2. validates the inner authenticated key against the decrypted descriptor `object_id`;
3. requires authenticated logical keys to be strictly increasing;
4. allocates one nonce from the retained-stage lease;
5. re-encrypts the 64-byte descriptor directly into the 92-byte retained stage.

Therefore changing an outer sort key in a way that reorders encrypted payloads is detected when the authenticated inner-key sequence is checked. An outer-key change that does not alter final authenticated logical ordering has no effect on canonical output.

## Canonical output equivalence

A 401-object regression compares the canonical source writer with two encrypted-sorter executions.

The two encrypted executions use the same key and operation identity but different private nonce prefixes.

The regression proves:

- both encrypted-sorter executions produce canonical UCOF output byte-for-byte;
- both return the canonical source-writer report exactly;
- sorter output payload is exactly `401 * 100` bytes;
- the sorter's final run read is exactly `401 * 108` bytes;
- the retained descriptor stage is exactly `401 * 92` bytes;
- spill-output SHA-256 differs across the two private executions;
- retained-stage ciphertext SHA-256 also differs;
- both spill and retained leases are exhausted exactly;
- durable nonce high-water state advances by exactly `802` counters.

Private encryption therefore remains independent of UCOF compatibility bytes and public writer reports.

## Duplicate detection

A multi-run regression supplies logical keys:

`1, 3, 2, 4, 2`

with two records per initial run.

The existing bounded sorter still rejects the duplicate during merge as `duplicate spill key 2`.

The regression also proves:

- canonical output remains untouched;
- all five spill nonces are already burned because every logical input was encrypted before merge;
- the retained-stage lease remains completely unused because final output never begins;
- private run files are cleaned up.

This preserves fail-closed duplicate semantics without moving duplicate authority into the encryption layer.

## Private-storage quota accounting

Experiment 0171 removes the plaintext retained-descriptor stage from the private-storage lifetime.

The important new peak is:

`encrypted sorter live-spill cap + growing encrypted retained descriptor stage`

followed by:

- retained encrypted descriptors + locator stage;
- locator + first page-reference stage;
- adjacent page-reference stages.

For a four-record executable geometry:

- encrypted sorter frame: 108 bytes;
- exact four-record sorter live-spill cap: 432 bytes;
- retained descriptor stage: 368 bytes;
- locator stage: 288 bytes;
- sorter + retained overlap: 800 bytes;
- retained + locator overlap: 656 bytes;
- required private capacity: 800 bytes.

The executable quota regression proves:

- exact 800-byte private capacity succeeds;
- 799 bytes fails before writer I/O;
- the one-byte-short failure consumes neither spill nor retained-stage nonces.

All generic sorter byte limits continue to operate on actual encrypted frame sizes because its `record_bytes` is changed from 64 to the 100-byte encrypted payload width while every byte cap is preserved.

## Short-lease preflight

A four-object regression supplies a spill lease containing only three counters.

The writer rejects the operation before creating sorter/private-stage files and before consuming either lease.

This proves encryption capacity is an operation precondition rather than a partial-write failure discovered mid-sort.

## What this closes

Experiment 0171 establishes executable evidence that:

- descriptor contents are encrypted before they enter bounded-sorter temporary files;
- merge passes never require plaintext descriptor spill;
- merge geometry does not amplify nonce consumption;
- the existing sorter retains ordering and duplicate authority;
- authenticated logical ordering is rechecked at final output;
- the final sorted descriptor flows directly into the retained encrypted stage without a plaintext retained copy;
- canonical UCOF bytes/reports remain independent of private spill encryption;
- encrypted sorter and retained-stage overlap is charged to private-storage quota.

## What remains open

Issue #11 remains open. Experiment 0171 does **not** establish:

- confidentiality of object identifiers/order keys in sorter runs;
- encrypted locator stages — 72-byte locator records remain plaintext private staging;
- encrypted page-reference stages — 64-byte page-ref records remain plaintext private staging;
- production key provisioning or key identity persistence;
- persistent authenticated storage of nonce prefixes, lease high-water state, operation/generation identity, or anti-rollback state;
- a real crash-durable journal implementation around the Experiment 0157 lease contract;
- physical power-loss/filesystem durability qualification;
- closure of the documented Linux same-UID final identity-check -> unlink race;
- AWS-LC provider qualification outside the native Linux x86_64 experiment.

## Verification

Accepted implementation head before this evidence note:

`c7b58f1907ceafcebe832f91c320a0e837ed1191`

Repository Rust workflow run:

`31792954212`

The implementation head passed:

- locked dependency-graph verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0171 regressions;
- Rust 1.85.0 and HTTP-feature MSRV;
- i686 generic-writer portability;
- powerpc64 generic-writer portability;
- concrete/async HTTP source checks;
- versioned S3 source checks;
- deletion-policy reproduction, reward, measurement, and catalog checks;
- Rust documentation;
- independent/adversarial parser and Phase 3 model checks;
- EXP-0002/EXP-0003 corpus and amendment verification;
- framing experiment replay.

## Next executable slice

Experiment 0172 should encrypt the remaining tree-construction private stages:

1. locator records — 72 plaintext bytes -> 100-byte AES-GCM frame;
2. page-reference records — 64 plaintext bytes -> 92-byte AES-GCM frame.

Unlike the sorter, those stages are consumed sequentially and do not need visible ordering metadata. The preferred design is a reusable fixed-record encrypted-stage adapter with:

- one nonce per persisted record;
- stage-kind and sequence bound in AAD;
- full-record encryption with no clear record fields;
- decrypting sequential readers feeding the existing bounded grouping/tree logic;
- exact-end authentication before a consumed stage is retired;
- revised overlap quota using encrypted locator/page-ref widths;
- canonical byte/report equivalence against Experiment 0171.

After locator/page-ref encryption, the main writer-security blockers narrow to persistent authenticated key/nonce/journal provenance, anti-rollback, real durability qualification, and the Linux final unlink race/isolation decision.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
