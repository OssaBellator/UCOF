# Experiment 0176 — Unified encrypted private-storage lifecycle

**Status:** non-normative Phase 3 implementation candidate; acceptance pending repository CI  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0152, 0171–0175

## Purpose

Experiments 0171–0175 establish individual encrypted/private lifecycle slices: bounded encrypted spill, durable nonce journals, authenticated restart-stage manifests, fresh-lease crash continuation, durable restart publication, and crash-authoritative retirement.

Before this experiment, storage limits were still split across those slices. The encrypted spill candidate priced sorter, retained-descriptor, locator, and page-reference overlap, but did not price the complete persistent/restart/publication lifecycle under one hard capacity.

Experiment 0176 consolidates those dimensions into one accounting model and moves the crash-resume quota check in front of the first fresh durable side effect.

## Accounting authority

The model reuses `EncryptedSpillPrivateStoragePlan`; it does not define a second sorter/tree geometry.

For the current research implementation the relevant fixed-width private records are:

- encrypted sorted descriptor spill frame: existing `ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES`;
- retained encrypted descriptor frame: existing `ENCRYPTED_DESCRIPTOR_STAGE_BYTES`;
- locator stage record: existing `LOCATOR_STAGE_BYTES`;
- page-reference stage record: existing `PAGE_REF_STAGE_BYTES`;
- nonce journal generation: existing `LINUX_NONCE_JOURNAL_BYTES`;
- encrypted restart-stage manifest: existing `ENCRYPTED_STAGE_MANIFEST_BYTES`;
- retirement record: existing `ENCRYPTED_RETIREMENT_BYTES`.

The plan therefore inherits widths from executable code that already parses/authenticates those records instead of copying numeric constants into a separate planner.

## Persistent inventory

`EncryptedPrivatePersistentInventory` records the bounded count of:

- authenticated nonce-journal generations already present;
- authenticated encrypted-retirement records already present.

For restart enforcement, inventory is discovered from the descriptor-pinned journal directory before a fresh lease is committed. Nonce generations are obtained through the existing authenticated journal scan. Retirement-shaped entries are opened, exact-end checked, HMAC-verified, decoded, and required to match the active key/nonce-prefix context before their count is accepted.

This preserves the existing fail-closed rule: malformed or foreign lifecycle metadata is not silently priced as trusted restart state.

## Normal encrypted path

The normal-path plan prices these overlap windows:

1. existing persistent inventory + the newly committed nonce generation + bounded encrypted sorter live spill/final spill;
2. existing persistent inventory + working final encrypted spill + durable restart-stage copy;
3. the same restart-copy window plus the authenticated stage manifest;
4. durable restart stage + manifest + working encrypted spill -> retained encrypted descriptor transcode;
5. durable restart stage + manifest + full private output staging + the largest post-preflight retained/locator/page-ref working window.

The normal required byte count is the maximum of those windows.

The durable restart copy is deliberately charged separately from the working final encrypted spill because `persist_sorted_encrypted_spill_restart_stage` copies the verified final spill into descriptor-pinned durable storage before authority is represented by its manifest.

## Crash-resume path

At restart the old durable encrypted spill and manifest are already present. The crash-resume plan prices:

1. existing authenticated persistent inventory before a fresh lease;
2. the same inventory plus exactly one new durable nonce-journal generation;
3. old durable encrypted spill + manifest + freshly transcoded retained encrypted descriptors;
4. old durable encrypted spill + manifest + full private output staging + the largest retained/locator/page-ref working window;
5. durable `Prepared` retirement authority while the old spill, manifest, and private output can still coexist;
6. durable `Terminal` retirement authority after old spill/manifest removal, while append-only journal/retirement records and private output may remain.

The crash-resume required byte count is the maximum of those windows.

## Unified lifecycle cap

`UnifiedEncryptedPrivateStoragePlan` derives:

- one normal encrypted publication plan from inventory present before the initial operation;
- one crash-resume plan from the inventory that would exist after the initial nonce generation became durable;
- one required byte cap equal to the maximum of both paths.

This gives one pre-computable hard bound for a complete operation that might either publish normally or later resume from its durable encrypted restart checkpoint.

The planner uses checked arithmetic for every count, width, and overlap sum. Zero output and overflowed inventory are rejected.

## Side-effect ordering improvement

`stage_and_publish_verified_encrypted_restart_with_private_quota` performs the following before the existing restart continuation path is entered:

1. authenticated bounded persistent-inventory scan;
2. canonical output-size prediction from source metadata and current immutable limits;
3. crash-resume lifecycle plan derivation;
4. hard-cap comparison.

Only after those steps succeed may the function call `stage_and_publish_verified_encrypted_restart`, whose first restart mutation is the fresh nonce-lease journal commit.

The exact-cap regression proves:

- a one-byte-short cap returns `encrypted restart private storage limit`;
- the durable nonce-journal recovery state is byte-for-byte unchanged after rejection;
- the private work directory remains empty;
- retrying the same fixture with the exact computed cap succeeds through durable publication;
- only the successful exact-cap path advances the nonce journal by one generation.

A separate regression proves the persistent inventory reports 0 -> 1 -> 2 retirement records across no-retirement, durable `Prepared`, and durable `Terminal` states while preserving the two nonce generations from crash + fresh continuation.

## Conservative choices

The output window charges the complete predicted private output together with the maximum post-preflight tree-stage overlap. This is intentionally conservative: the implementation streams output while intermediate stages change lifetime, but a production quota must not depend on optimistic byte-by-byte interleaving unless that tighter lifetime proof is executable.

The retirement windows continue to charge the private output after durable publication because `retire_private` can fail and the private staging name may remain. Hard-link publication does not duplicate filesystem blocks, but the private lifecycle still has to tolerate the retained private name and any quota policy that accounts it.

## What this closes

Experiment 0176 closes the arithmetic/sequencing gap identified by Experiment 0175 for the current encrypted restart/publication research path:

- sorter/final-spill, durable restart-copy, manifest, retained descriptors, locators, page refs, output staging, nonce journals, and retirement records share one accounting vocabulary;
- normal and crash-resume paths can be compared under one hard capacity;
- restart quota rejection can happen before fresh nonce authority or private-output mutation;
- exact-cap and one-byte-short behavior are executable rather than prose-only.

## What remains open

Issue #11 remains open. Experiment 0176 does **not** establish:

- filesystem free-space reservation or immunity from unrelated concurrent disk consumption;
- physical power-loss/filesystem qualification of `sync_all` ordering;
- a stronger isolation mechanism closing the Linux same-UID final identity-check -> unlink race;
- bounded compaction/reclamation policy for append-only nonce and retirement journals;
- durable binding of the external source set/order used by restart continuation;
- production AES/HMAC key provisioning or a non-rollbackable freshness anchor;
- encryption of locator/page-reference private stages on this consolidated restart branch;
- confidentiality of clear sorter object IDs or spill geometry;
- production qualification outside the native Linux x86_64 crypto evidence;
- a stable or accepted EXP-0003 wire format.

The planner accounts for existing append-only journal records, but it does not make unbounded growth acceptable. Production still needs a compaction/retention policy with its own crash-authoritative transition.

## Verification

Repository CI for the Experiment 0176 branch is the acceptance authority. This document must be updated with the accepted implementation head and workflow run only after the relevant Rust/Phase 3 gates complete successfully.

## Next consolidation slice

After 0176 is green, the highest-value Phase 3 writer work is no longer another independent crypto primitive. The next slice should bind restart continuation to an authenticated durable source-set/order manifest so that a valid old encrypted descriptor stage cannot be continued against a different external source list that merely has the same object count.

That source-set binding should be designed to coexist with journal compaction rather than creating another append-only namespace that must later be reconciled.

## Governance boundary

This remains implementation/storage evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor wire bytes, or make a compatibility promise.
