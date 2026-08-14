# Experiment 0172 — Encrypted locator and page-reference staging

**Status:** non-normative Phase 3 production-writer confidentiality evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0157, 0170, 0171

## Purpose

Experiment 0172 encrypts the remaining tree-construction private stages in the bounded source-writer path. After 0171, source descriptors were encrypted before external sorting and the retained descriptor stage was encrypted, but locator and page-reference staging still persisted plaintext records.

0172 replaces those plaintext stages with a reusable sequential AES-256-GCM fixed-record adapter while preserving the existing canonical grouping, page encoding, snapshot, footer, and source-writer semantics.

## Encrypted stage geometry

The reusable adapter encrypts complete fixed-width records; no record fields remain clear on disk.

- locator: 72 plaintext bytes -> 100-byte frame;
- page reference: 64 plaintext bytes -> 92-byte frame.

Each frame is:

- 12-byte nonce;
- exact plaintext-width AES-256-GCM ciphertext;
- 16-byte authentication tag.

AAD binds:

- `UCOF-EXP-0172-TREE-STAGE\0` domain separation;
- stage kind (`Locator` or `PageRef`);
- stage ordinal;
- operation identity;
- durable journal generation;
- record sequence;
- leased nonce counter;
- exact plaintext record width.

The ordinal makes otherwise-same-width page-reference records non-transferable between tree levels.

## Nonce authority

One exact tree lease covers every persisted locator and page-reference record for the operation.

The required count is computed from the canonical tree shape before sorter or tree-stage creation:

`object_count locator records + every page-ref stage record, including the final root reference`

The top-level encrypted-tree writer rejects an undersized tree lease before the existing encrypted sorter runs and before any private-stage file is created.

This tree lease is disjoint from 0171's spill and retained-descriptor leases.

## Tree execution path

During object emission:

1. the 0171 retained encrypted descriptor stage is authenticated/read;
2. canonical object bytes are written;
3. each resulting locator is immediately encrypted into the 100-byte locator stage.

After all objects:

1. the descriptor reader reaches exact end;
2. the encrypted locator stage is authenticated;
3. the retained descriptor stage is retired;
4. locator records are decrypted sequentially into the existing bounded leaf groups;
5. canonical leaf pages are written;
6. resulting page refs are encrypted into the first 92-byte page-ref stage;
7. each later page-ref level is decrypted sequentially, grouped with the existing canonical policy, and emitted into a newly encrypted next-level stage;
8. the final one-record encrypted page-ref stage yields the canonical root.

The in-memory bounds remain the existing one-group locator/page-ref bounds. Encryption does not introduce a workload-sized plaintext collection.

## Authentication and exact-end behavior

Every encrypted stage is authenticated and exact-end checked before it is retired.

The reusable reader requires:

- exact nonce sequence;
- valid AES-GCM authentication;
- exact decrypted width;
- exact configured record count;
- exact file end after the final record.

A direct tamper regression writes one encrypted locator-sized record, verifies it, flips a ciphertext byte, and proves authentication fails. The consumed nonce remains burned.

## Canonical output equivalence

A 401-object regression compares the canonical source writer with two fully encrypted-private-stage executions.

Both executions use the same key and operation identity but different private nonce prefixes.

The regression proves:

- canonical UCOF output bytes are identical to the existing source writer;
- canonical source-writer reports are identical;
- encrypted sorter ciphertext differs between executions;
- retained descriptor ciphertext differs;
- aggregate encrypted tree-stage ciphertext differs;
- spill, retained, and tree leases are each exhausted exactly;
- durable nonce high-water state advances by `2 * object_count + exact_tree_record_count`.

Private encryption therefore remains outside compatibility bytes and public writer reports.

## Private-storage quota

0172 prices the real stage lifetimes rather than summing all private artifacts.

The important overlap windows are:

- encrypted sorter live-spill + retained encrypted descriptor;
- retained encrypted descriptor + encrypted locator;
- encrypted locator + first encrypted page-ref stage;
- adjacent encrypted page-ref levels.

For the executable four-object geometry:

- encrypted sorter frame: 108 bytes;
- sorter live-spill cap: 432 bytes;
- retained descriptor stage: 368 bytes;
- encrypted locator stage: 400 bytes;
- first encrypted page-ref stage: 92 bytes;
- sorter + retained overlap: 800 bytes;
- retained + locator overlap: 768 bytes;
- locator + leaf-ref overlap: 492 bytes;
- maximum adjacent page-ref overlap: 92 bytes;
- required private capacity: 800 bytes.

The regression proves exact 800-byte capacity succeeds, while 799 bytes fails before writer I/O and before any of the three leases are consumed.

## What this closes

Experiment 0172 establishes executable evidence that the bounded writer can keep all of these private record payloads encrypted at rest while preserving canonical output:

- external-sort descriptor payloads;
- retained sorted descriptors;
- locators;
- page references at every tree level.

The generic sorter and canonical tree/page logic remain the existing implementations.

## What remains open

Issue #11 remains open. In particular, 0172 does **not** establish:

- confidentiality of the sorter's clear ordering keys/object IDs;
- production key provisioning or key identity lifecycle;
- real keyed authentication for the restart journal;
- persistent authenticated binding of operation/key identity, nonce prefix, nonce high-water state, and private-artifact inventory;
- a real crash-durable journal storage implementation;
- anti-rollback backed by durable storage rather than caller-provided minimum state;
- physical power-loss/filesystem durability qualification;
- closure of the documented Linux same-UID final identity-check -> unlink race;
- AWS-LC provider qualification outside the native Linux x86_64 experiment.

## Verification

Accepted implementation head before this evidence note:

`6eff0df14ea8b97906f314164b0f300421d8e60a`

Repository Rust workflow run:

`31794020206`

The implementation head completed successfully on:

- locked dependency-graph verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0172 regressions;
- Rust 1.85.0 and HTTP-feature MSRV;
- i686 generic-writer portability;
- powerpc64 generic-writer portability;
- concrete and async HTTP source checks;
- versioned S3 source checks;
- deletion-policy reproduction/measurement/catalog checks;
- Rust documentation;
- independent/adversarial parser and Phase 3 model checks;
- EXP-0002/EXP-0003 corpus and amendment verification;
- framing experiment replay.

## Next executable slice

Experiment 0173 should replace the restart journal's test-only authenticator with a real keyed MAC while reusing the existing canonical journal bytes and rollback fields.

HMAC-SHA256 is preferred over journal AEAD for this slice because journal authentication does not require another crash-safe nonce stream. The existing journal already carries operation identity, key identity, generation, nonce high-water state, publication authority, and bounded artifact inventory.

0173 should prove real HMAC verification, field/tag tamper rejection, truncation rejection, foreign-key rejection, and authenticated-old-generation rollback rejection through the existing minimum-generation/minimum-nonce checks.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
