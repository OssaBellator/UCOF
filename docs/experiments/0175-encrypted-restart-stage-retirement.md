# Experiment 0175 — Encrypted restart stage retirement

**Status:** non-normative Phase 3 crash-authoritative private-retirement evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0163–0166 and 0172–0174

## Purpose

Experiment 0174 can continue from a strongly verified crashed encrypted spill under a fresh durable nonce lease and reproduce canonical public bytes/reports. It deliberately leaves the old encrypted restart stage and its durable manifest in place.

Experiment 0175 adds the destructive boundary for retiring those old private objects. Cleanup authority is not inferred from successful computation or a link attempt. It is minted only after the public artifact has crossed the existing private-publication durability boundary: private validation, private sync, no-replace link, and destination-parent sync.

## Durable-publication gate

The encrypted restart publication layer has three outcomes:

- `NotPublishedDestinationExists`;
- `PublicationIndeterminate` at link or parent-sync authority;
- `PublishedAndDurable`, carrying the canonical continuation evidence plus exact output length/SHA-256.

Only `PublishedAndDurable` can be passed to retirement preparation. Destination-exists and parent-sync-indeterminate outcomes have no retirement-capable evidence object.

A fresh continuation nonce lease may already be burned when publication later fails or is not selected; this is conservative and safe, but wasteful. It does not authorize deletion of the crashed stage.

## Retirement journal

Retirement uses a namespace separate from the nonce high-water journal so cleanup terminality cannot be confused with nonce authority.

Each authenticated retirement record is 208 bytes:

- 176-byte canonical body;
- 32-byte HMAC-SHA256 tag under the Experiment 0172 journal authentication key.

It binds:

- state: `Prepared` or `Terminal`;
- AES key identity and nonce prefix;
- crashed generation;
- fresh continuation generation;
- strong encrypted-stage identity;
- strong stage-manifest identity;
- durably published output length;
- durably published output SHA-256.

`Prepared` is written with create-new semantics, file `sync_all`, and journal-directory `sync_all` before any unlink can occur.

## Destructive execution

Execution requires a valid durable `Prepared` record. Without it, the result is `NoPreparedAuthority` and no deletion is attempted.

Before the first unlink, the executor completely classifies **both** cleanup targets:

1. the persisted encrypted spill in the pinned stage directory;
2. the persisted encrypted-stage manifest in the pinned journal directory.

For each target, only these states are actionable:

- exact expected strong identity;
- exact strong identity found under another name;
- complete bounded scan proving absence.

Expected-name replacement, unreadable identity, bounded-scan truncation, or other conflicting evidence returns `RetainIndeterminate` and blocks **all** destructive work.

Immediately before unlink, the chosen path is reopened through the pinned directory and its strong identity is recomputed and required to match the prepared record. The actual unlink still has the repository's documented same-UID final-check-to-unlink race; this experiment does not claim stronger isolation than the preceding Linux evidence.

## Commit ordering

The cleanup sequence is:

`durable public publication -> durable Prepared -> classify both -> final identity checks -> unlink stage/manifest -> stage-dir sync -> journal-dir sync -> durable Terminal`

A `Terminal` record is create-new, file-synced, and journal-directory-synced. If Terminal is already present on restart, execution returns `AlreadyTerminal` and never blindly retries deletion.

## Crash-cut evidence

Executable regressions cover:

- no durable Prepared record: no destructive authority;
- durable Prepared before unlink: restart retries exact cleanup;
- stage removed before directory sync: restart treats proven absence conservatively, completes the remaining manifest cleanup, syncs directories, then finalizes;
- both targets removed and directories synced before Terminal: restart finalizes without requiring another delete;
- exact stage renamed after preparation: cleanup resolves and removes it by strong identity;
- expected stage pathname replaced while the original exact object exists elsewhere: both stage and manifest cleanup are blocked as indeterminate;
- manifest bytes changed after preparation: stage cleanup is also blocked because both targets are classified before either unlink;
- terminal replay: no repeated destructive action.

## Publication regressions

The same accepted branch proves:

- parent-synced publication reproduces the canonical bytes/report and returns the only durable retirement-capable payload;
- destination-exists preserves the existing destination and returns no durable retirement authority;
- parent-sync failure is explicitly indeterminate and retains private publication state without creating cleanup authority.

## What this closes

Experiment 0175 establishes executable evidence that old encrypted restart evidence is not destructively retired until:

- a fresh continuation has been privately staged and durably published;
- exact output identity is bound into a durable cleanup record;
- stage and manifest identities are both known before the first unlink;
- directory durability precedes terminal cleanup authority;
- crash/restart can retry or finalize without pathname guessing or blind repeated deletion.

## What remains open

Issue #11 remains open. Experiment 0175 does **not** yet establish:

- one unified private-storage quota covering restart copy, manifests, retirement records, fresh retained stage, publication staging, locator/page-reference stages, and overlap windows;
- physical power-loss/filesystem qualification of the sync ordering;
- a stronger isolation mechanism closing the same-UID final identity-check → unlink race;
- journal/retirement compaction after bounded append-only growth;
- durable binding of the external source set/order used by restart continuation;
- production AES/HMAC key provisioning or a non-rollbackable freshness anchor;
- encryption of locator/page-reference private stages;
- confidentiality of clear sorter object ids or spill geometry;
- production qualification outside the native Linux x86_64 crypto experiment.

A production candidate should also decide whether old restart evidence may be retained intentionally for forensic policy rather than always retired after durable publication; 0175 proves safe retirement authority, not mandatory retention policy.

## Verification

Accepted implementation head before this evidence commit:

`fd8e0b048ee6503d3089fe99cce80301a2df77ce`

Acceptance workflow run:

`31803318510`

The acceptance gate completed successfully on:

- locked dependency metadata;
- workspace formatting;
- Clippy with warnings denied;
- full Rust workspace/all-target implementation tests;
- Rust 1.85.0 workspace/all-target compatibility.

Generic i686/powerpc64 portability remains enforced by the normal repository Rust workflow on this branch; the AWS-LC implementation stays native Linux x86_64 test evidence.

## Next executable slice

The highest-value next slice is a **unified production private-storage plan** for the complete encrypted restart/publication lifecycle.

It should price, under one hard capacity before work begins:

- bounded sorter live spill and final encrypted spill;
- durable restart-stage copy;
- stage manifest and nonce/retirement journals;
- old encrypted stage + fresh retained encrypted descriptors during resume;
- retained descriptors + locators;
- locator/page-reference tree levels;
- private public-output staging;
- cleanup overlap while old restart evidence remains durable.

Exact-cap and one-byte-short tests should cover both normal encrypted publication and crash-resume publication. Only after that capacity model is executable should additional private-stage encryption outrank storage/durability qualification.

## Governance boundary

This remains private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor wire bytes, or make a compatibility promise.
