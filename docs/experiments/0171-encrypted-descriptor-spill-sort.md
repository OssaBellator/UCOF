# Experiment 0171 — Encrypted descriptor spill sort

**Status:** non-normative Phase 3 production-writer confidentiality evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0157 and 0170

## Purpose

Experiment 0170 encrypted the retained sorted descriptor stage but deliberately left the bounded external sorter's temporary run payloads in plaintext.

Experiment 0171 closes that descriptor-payload gap without creating a second sorter implementation. Source descriptors are encrypted before they enter the existing bounded sorter. The sorter continues to order, merge, duplicate-check, account for, and clean up opaque fixed-width payloads exactly as before.

The resulting writer path is:

`source metadata -> 64-byte descriptor -> lease-backed AES-GCM spill payload -> existing bounded external sort -> sorted encrypted spill stage -> authenticated in-memory transcode -> 92-byte retained encrypted descriptor stage -> existing shared canonical emission loop`

This experiment deliberately does **not** claim that every private staging byte is confidential. The outer object-id sort key remains visible, spill/run geometry remains observable, and locator/page-reference stages remain separate follow-on gates.

## Why the sorter itself was not forked

Experiment 0170 proposed a frame-codec seam that would encrypt every persisted sorter frame, including fresh encryption of merge outputs.

Inspection of the current sorter showed a narrower construction is sufficient for descriptor payload confidentiality: the sorter treats the fixed payload as opaque and copies it unchanged through merge passes while ordering exclusively by the separate `u64` key.

Experiment 0171 therefore encrypts each logical descriptor once before the sorter sees it. Merge passes move that ciphertext unchanged. This has three useful consequences:

- all sorter-run descriptor payloads are encrypted without duplicating merge/order logic;
- merge fan-in and merge-pass count do not multiply nonce consumption;
- the existing sorter remains the only implementation of duplicate detection, ordering, spill cleanup, and merge accounting.

The visible sort key is still an explicit confidentiality boundary, not an accidental omission.

## Encrypted spill payload

Each logical 64-byte source descriptor becomes one fixed 100-byte sorter payload:

- 8-byte authenticated copy of the object id;
- 12-byte nonce;
- 64-byte AES-256-GCM ciphertext;
- 16-byte authentication tag.

The bounded sorter still prepends its existing 8-byte clear ordering key, so each persisted sorter frame is 108 bytes.

The authenticated embedded object-id copy is intentional. The clear outer key controls sorting, while the embedded copy is protected by AEAD and is compared against the decrypted descriptor before transcode acceptance.

AAD binds:

- the Experiment 0171 spill-descriptor domain;
- operation identity;
- durable journal generation;
- object id;
- nonce counter;
- exact 64-byte plaintext descriptor width.

The spill nonce is the same four-byte session prefix plus leased 64-bit big-endian counter construction established in Experiment 0170.

## Nonce accounting

Experiment 0171 uses the existing opaque `DescriptorEncryptionSession`, which itself can exist only after Experiment 0157 lease activation.

Exactly two nonce counters are required per source descriptor:

1. one counter when the descriptor is encrypted before entering the sorter;
2. one counter when the sorted spill descriptor is authenticated/decrypted in memory and re-encrypted into the retained 92-byte descriptor stage.

Sorter merge passes consume no additional nonces because they copy the already-encrypted opaque payload unchanged.

The preflight therefore requires an active lease with at least `2 * object_count` remaining counters before descriptor work begins.

A 401-object equivalence regression proves the exact 802-counter lease is exhausted and the durable high-water authority advances across the whole committed lease.

## Sorted spill validation and transcode

The final sorted sorter payload stream is retained temporarily as fixed 100-byte records.

Before canonical output begins, Experiment 0171:

1. verifies the exact fixed-stage byte count;
2. requires strict increasing embedded object ids;
3. checks the nonce prefix and derives the leased counter from each nonce;
4. authenticates/decrypts each 64-byte descriptor payload;
5. decodes the descriptor and requires its object id to equal the authenticated embedded object id;
6. allocates the next retained-stage nonce;
7. re-encrypts the descriptor directly into the existing Experiment 0170 92-byte retained stage.

Plaintext descriptor material exists only in memory during this transcode; no new plaintext descriptor stage is persisted.

The retained stage then receives Experiment 0170 full-stage authenticated prevalidation before the first UCOF output byte and is consumed by the same shared canonical descriptor-reader emission loop.

## Canonical output equivalence

A 401-object regression compares the canonical source writer with two independently encrypted Experiment 0171 executions.

The encrypted executions use the same key and operation identity but different private nonce prefixes.

The regression proves:

- both encrypted executions produce the exact canonical UCOF bytes;
- both return the exact canonical source-writer report;
- each sorted encrypted spill stage is exactly `401 * 100` bytes;
- each retained encrypted descriptor stage is exactly `401 * 92` bytes;
- the bounded sorter reports exactly the encrypted spill payload bytes written to the sorted output stage;
- the two sorted private ciphertext-stage SHA-256 digests differ;
- the two retained encrypted descriptor-stage SHA-256 digests differ;
- public UCOF bytes and reports remain independent of private encryption randomness;
- the exact `2 * object_count` nonce lease is exhausted.

## Fail-closed corruption evidence

Three controlled mutation regressions act on the sorted encrypted spill stage before retained-stage transcode.

### Ciphertext mutation

One ciphertext byte is flipped.

The result is:

- AES-GCM authentication failure;
- zero public output bytes;
- no retained-stage nonce consumed before the failing record is accepted;
- temporary private state retired by stage ownership/drop cleanup.

### Record reorder

The first two fixed encrypted spill payloads are swapped.

The transcode rejects non-increasing authenticated embedded object ids before canonical output. A retained-stage nonce already consumed for a previously accepted record remains burned; it is not rewound or reused.

### Truncation

The sorted encrypted spill stage is shortened by one byte.

Exact fixed-stage byte validation fails before transcode and before public output.

These tests target the final sorted spill stage. Because merge passes copy encrypted payloads unchanged, payload corruption introduced in an intermediate run propagates to the same authentication boundary. A clear outer-key mutation that changes final ordering is constrained by the authenticated embedded object id and strict ordering/key-equality checks at transcode; this is structural evidence, not a claim that the visible outer key is confidential.

## Private-storage quota accounting

Experiment 0171 expands the private-storage plan to price encrypted sorter payloads and the transcode overlap explicitly.

The plan covers:

- configured maximum live sorter spill plus the final 100-byte-per-object encrypted spill stage;
- encrypted spill stage plus the 92-byte-per-object retained encrypted descriptor stage during authenticated transcode;
- retained encrypted descriptor stage plus the 72-byte-per-object locator stage during object emission;
- locator plus first page-reference stage;
- maximum adjacent page-reference levels.

For the executable four-object quota regression:

- encrypted spill descriptors: 400 bytes;
- retained encrypted descriptors: 368 bytes;
- locators: 288 bytes;
- first page-reference stage: 64 bytes;
- configured sorter live-spill cap: 432 bytes, exactly four 108-byte persisted sorter frames;
- sorter plus final encrypted spill stage: 832 bytes;
- encrypted spill plus retained transcode overlap: 768 bytes;
- retained descriptor plus locator overlap: 656 bytes;
- required private capacity: 832 bytes.

The exact 832-byte capacity succeeds. A capacity of 831 bytes fails before source/writer I/O and consumes no nonce from the active lease.

The sorter itself still enforces its existing read/write/live-spill budgets with the widened 100-byte logical payload, so its internal persisted frame accounting naturally uses 108 bytes per record.

## What this closes

Experiment 0171 establishes executable evidence that:

- clear descriptor payloads are no longer written into bounded-sorter run files;
- the existing sorter can remain the single ordering/merge implementation;
- encrypted payloads survive arbitrary bounded merge passes without re-encryption or nonce amplification;
- clear sort keys are authenticated indirectly against protected object identity before acceptance;
- retained-stage encryption from Experiment 0170 remains byte/report equivalent to the canonical writer;
- encrypted sorter expansion is included in private-storage quota planning;
- corruption, reorder, and truncation fail before public UCOF output.

## What remains open

Issue #11 remains open. Experiment 0171 does **not** establish:

- confidentiality of the outer object-id sort key or sort order;
- confidentiality of run count, run lengths, merge geometry, or other spill-access metadata;
- encrypted locator stages;
- encrypted page-reference stages;
- a production key-management or key-provisioning contract;
- persistent authenticated storage of key identity, nonce prefix, lease high-water state, expected private-stage identities, or anti-rollback state;
- a real crash-durable journal implementation that commits Experiment 0157 lease authority before the real AES-GCM operations;
- crash recovery of an interrupted encrypted spill/transcode operation against that real journal;
- physical power-loss/filesystem durability qualification;
- closure of the documented same-UID Linux final identity-check -> unlink race;
- AWS-LC provider qualification outside the native Linux x86_64 experiment.

There is also implementation duplication between the plaintext preflight and this encrypted-spill preflight. That is acceptable for a disposable experiment but should be factored into a shared metadata/output-size preflight before any production candidate is promoted, otherwise policy drift becomes a maintenance risk.

## Verification

Accepted implementation head before this evidence note:

`431e89f854327af0b28b015f1685a29d80656279`

Repository Rust workflow run:

`31798182665`

At the point this evidence note was written, the accepted implementation head had completed successfully on:

- locked dependency-graph verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0171 regressions;
- Rust 1.85.0 MSRV and HTTP-feature MSRV;
- i686 portability checks for the generic writer;
- powerpc64 portability checks for the generic writer.

The broader repository replay continues through the same HTTP/source, policy, documentation, independent parser/model, corpus, and framing checks used for Experiment 0170. The evidence note should be updated only if one of those broader checks exposes a regression.

## Next executable slice

The highest-value next slice is no longer another descriptor-encryption layer. The descriptor plaintext path is now covered through sorter spill and retained staging.

Two remaining directions are materially useful:

1. **Journal/nonce authority integration:** replace the Experiment 0157 durable-commit boundary with a real authenticated crash-durable journal that persists lease high-water state, key identity/prefix binding, and anti-rollback authority before any real seal operation.
2. **Remaining private-stage confidentiality:** encrypt locator and page-reference staging if the production confidentiality requirement is that no sensitive private working record is left in cleartext.

The journal integration should take priority because cryptographic confidentiality is unsafe to promote without crash-authoritative nonce state, whereas locator/page-reference encryption is primarily a scope/completeness question once the nonce authority is real.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
