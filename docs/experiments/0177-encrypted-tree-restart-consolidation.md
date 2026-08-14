# Experiment 0177 — Encrypted tree restart consolidation

**Status:** accepted non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0172, 0174–0176

## Purpose

Phase 3 had two partially overlapping post-0170 issue #11 lines: an earlier confidentiality stack with encrypted sorter/tree stages, and a later durability/restart stack with authenticated durable nonce journals, restart-stage manifests, fresh-lease continuation, publication, retirement, and unified lifecycle accounting.

Experiment 0177 consolidates the unique encrypted locator/page-reference staging evidence onto the durability/restart spine without reintroducing the older competing sorter or restart-journal implementation.

## Encrypted tree-stage adapter

The consolidated fixed-record adapter preserves the earlier confidentiality contract:

- locator plaintext width 72 bytes -> 100-byte encrypted frame;
- page-reference plaintext width 64 bytes -> 92-byte encrypted frame;
- AES-256-GCM through the existing `DescriptorEncryptionSession` key/nonce namespace;
- AAD binds stage kind, stage ordinal, operation identity, journal generation, sequence, nonce counter, and exact plaintext width;
- sequential nonce expectations are checked again when reading;
- every stage is authenticated and exact-end checked before its records are trusted;
- ciphertext tampering and a foreign session/nonce namespace fail closed.

The port uses the durability branch's existing encryption session rather than introducing a second AEAD/key abstraction.

## Exact tree nonce requirement

`consolidated_encrypted_tree_stage_record_count` derives the exact number of encrypted tree records before work:

- one nonce per locator/object;
- one nonce per leaf page reference;
- one nonce per page reference at every successive internal level through the single root reference.

A short tree lease is rejected before sorter work or public output begins. The exact lease is exhausted at the end of a successful tree build.

## Normal encrypted pipeline

The normal consolidated pipeline is:

`bounded encrypted descriptor spill -> retained encrypted descriptors -> encrypted locators -> encrypted page refs at every level -> canonical public output`

It reuses `PreparedDescriptorReader`, so plaintext/encrypted retained-descriptor parsing remains one seam rather than duplicating object emission logic.

Executable evidence with 401 reverse-ordered source objects proves:

- public bytes equal the canonical source writer exactly;
- the source-writer report is identical;
- changing the private nonce prefix changes encrypted descriptor spill, retained descriptor, and tree-stage ciphertext identities;
- private ciphertext randomness does not change canonical public bytes or reports.

## Encrypted tree lifecycle accounting

`ConsolidatedEncryptedTreeStoragePlan` replaces the plaintext locator/page-ref widths in the post-preflight lifecycle window with the actual encrypted frame widths.

It prices:

- retained encrypted descriptors + encrypted locators;
- encrypted locators + encrypted leaf references;
- every adjacent encrypted page-reference level overlap.

Normal and crash-resume lifecycle planners then feed the larger encrypted-tree working window back into Experiment 0176's output/private-storage maximum rather than maintaining a second quota model.

## Crash-resume combined lease

For a verified crashed encrypted spill, the fresh restart generation reserves one exact combined lease:

`retained-descriptor records + exact encrypted-tree records`

The ordering is:

1. classify and strongly verify the old durable restart stage and manifest;
2. compute exact tree nonce count;
3. commit one fresh journal generation for the combined range;
4. transcode the old encrypted spill into fresh retained encrypted descriptors, consuming the prefix;
5. emit encrypted locator/page-reference stages, consuming the remainder;
6. require the fresh session to be exactly exhausted.

If post-commit source validation fails, the committed range remains burned. It is never reused on restart.

If the durable stage manifest is absent, no combined fresh lease is committed.

## Durable publication and retirement reuse

The consolidated encrypted-tree continuation routes through the existing private publication boundary:

- private output validation;
- private file sync;
- no-replace publication;
- destination-parent sync;
- optional private-name retirement.

Only parent-synced publication yields durable cleanup-capable evidence.

The resulting authority is wrapped into the existing `DurableEncryptedRestartPublication` structure, so Experiment 0175's authenticated `Prepared`/`Terminal` retirement protocol remains the sole destructive cleanup authority. No second retirement format is introduced.

Executable regressions prove:

- canonical durable publication;
- destination-exists mints no retirement authority;
- parent-sync failure remains publication-indeterminate and mints no retirement authority;
- the existing Prepared/Terminal retirement path can retire the consolidated encrypted-tree restart evidence.

## What this closes

Experiment 0177 closes the main confidentiality divergence between the earlier tree-encryption branch and the newer durability/restart spine:

- locators and every page-reference private stage are encrypted on the consolidated path;
- exact tree nonce sizing is precomputed and enforced;
- crash-resume uses one durable generation for retained descriptors plus encrypted tree stages;
- encrypted tree-stage widths participate in the unified lifecycle quota;
- publication and retirement reuse the accepted durability-line authority model;
- the older divergent sorter/HMAC implementations are not required by the consolidated path.

## What remains open

Issue #11 remains open. Experiment 0177 does **not** establish:

- durable binding of the exact external source resource set/order used by restart continuation;
- filesystem free-space reservation or protection from unrelated concurrent disk use;
- physical power-loss/filesystem qualification of the sync sequence;
- bounded compaction/reclamation of append-only nonce/retirement metadata;
- a stronger mechanism closing the Linux same-UID final identity-check -> unlink race;
- production key provisioning, rotation, or a non-rollbackable freshness anchor;
- confidentiality of clear sorter object IDs or spill geometry;
- production qualification outside native Linux x86_64 crypto evidence;
- a stable or accepted EXP-0003 wire format.

`strong_version` continues to identify an immutable payload view; it is not treated as an external resource/provenance identity.

## Verification

Accepted implementation head before this evidence commit:

`935c969297619d8c05226d216ad7724b9545b0d9`

Repository Rust workflow:

`31805819537` — fully green, including formatting, Clippy with warnings denied, full Rust implementation tests, Rust 1.85.0, i686, powerpc64, concrete HTTP/S3 assurance tests, policy/corpus checks, independent-model checks, documentation, and framing replay.

The immutable-successor vector workflow was also green on the accepted code head.

## Next consolidation slice

The next highest-value restart-authority slice is an authenticated durable binding to a caller-supplied opaque identity for the exact external source resource set/order. That authority must be verified before a fresh restart nonce generation is committed and must be charged to the persistent private-storage lifecycle.

Core should not fabricate provider resource identity from `strong_version`; applications/providers own the mapping from resources to the opaque source-set identity.

## Governance boundary

This remains implementation/storage evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor public wire bytes, or make a compatibility promise.
