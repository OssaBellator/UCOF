# Experiment 0178 — Restart source-set authority

**Status:** accepted non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0174–0177

## Purpose

Experiments 0174–0177 can verify a durable encrypted restart stage, reserve a fresh crash-safe descriptor+tree nonce range, reproduce canonical public bytes, publish durably, and retire old private restart evidence.

One authority gap remained: the persisted source descriptors authenticate object metadata and immutable payload versions, but they do not identify the external resources from which those payloads are obtained. A different resource set can legitimately expose the same object metadata or provider version-token shape.

Experiment 0178 therefore adds an explicit durable binding to a caller-supplied opaque identity for the exact external source resource set/order authorized for restart continuation.

## Ownership boundary

Core does not invent provider resource identity from `ObjectId`, payload bytes, `strong_version`, URLs, paths, bucket/key names, or any other provider-specific naming rule.

The caller/provider supplies one opaque nonzero 32-byte `source_set_id` whose semantics are outside Core. It can represent, for example, a canonical digest of provider resource identities/order under an application policy.

UCOF only authenticates, persists, compares, and lifecycle-prices that opaque identity.

A matching `source_set_id` does not by itself prove freshness, authorization, provenance, or that the caller's identity construction is collision-resistant. Those remain caller/provider policy claims.

## Source-set authority record

The append-only authority record is 176 bytes:

- 144-byte canonical body;
- 32-byte HMAC-SHA256 tag using the existing restart-journal authentication key.

It binds:

- restart-stage role;
- journal key identity;
- nonce prefix;
- original operation identity;
- durable generation;
- exact authenticated restart-stage manifest identity;
- caller-supplied 32-byte source-set identity;
- exact object count.

The filename is canonical for generation + stage role. Loading requires:

- descriptor-pinned private journal directory;
- regular exact-length file;
- exact end;
- valid HMAC;
- canonical name;
- matching generation/role;
- matching journal key/nonce context.

## Durable ordering

Source-set authority is created only after the encrypted restart stage and its authenticated stage manifest are already durable.

The source-set record uses:

`create_new -> write -> flush -> file sync -> journal-directory verification -> journal-directory sync -> source-bound restart authority`

A valid stage + stage manifest can therefore exist without source-set authority. That state remains valid encrypted restart evidence for the older restart model, but it is deliberately **not** enough to mint a source-bound fresh continuation generation.

## Restart ordering

The source-bound encrypted-tree continuation performs this sequence before fresh nonce authority:

1. classify the encrypted restart stage;
2. strongly verify the exact stage identity against its authenticated manifest;
3. load and authenticate the source-set authority;
4. require exact stage identity, object count, journal context, and caller `source_set_id` match;
5. only then recover nonce authority and durably commit the fresh combined descriptor+encrypted-tree nonce lease;
6. transcode retained descriptors and emit encrypted locator/page-reference stages through Experiment 0177.

Missing, wrong, or tampered source-set authority therefore cannot burn or advance a fresh generation.

## Executable negative evidence

Regressions prove:

- stage + manifest but no source-set authority -> restart fails with generation 1 still authoritative;
- wrong caller source-set identity -> restart fails before generation advancement;
- HMAC-tampered source-set record -> authentication failure before generation advancement;
- exact valid source-set identity -> canonical encrypted-tree restart continuation and fresh generation 2;
- a source-set authority record round-trips only when its exact restart-stage manifest identity and journal context agree.

## Lifecycle quota integration

Source-set authority is not treated as free metadata.

`SourceBoundPersistentInventory` extends the Experiment 0176/0177 private-storage inventory with authenticated source-set record count.

The source-bound normal path charges:

- existing source-set authority bytes;
- one new 176-byte source-set record after the stage manifest becomes durable;
- source-bound transcode and private-output overlap on top of the encrypted-tree lifecycle widths.

The source-bound crash-resume path charges existing source-set authority bytes during:

- fresh retained-descriptor transcode;
- encrypted-tree/private-output staging;
- Prepared retirement overlap;
- Terminal retirement overlap.

The exact-cap regression proves a one-byte-short source-bound crash-resume quota fails before generation 2, while the exact computed cap succeeds.

## What this closes

Experiment 0178 closes the restart resource-identity gap at the boundary Core can safely own:

- exact external source set/order is represented by an explicit caller-owned opaque identity;
- that identity is authenticated and bound to the exact durable encrypted restart-stage identity;
- source identity is checked before fresh nonce authority;
- source-set records participate in the bounded persistent private-storage lifecycle;
- no provider-specific identity semantics are smuggled into `strong_version`.

## What remains open

Issue #11 remains open. Experiment 0178 does **not** establish:

- a universal method for deriving `source_set_id` across providers/applications;
- provider freshness, authorization, provenance, or credential continuity;
- bounded reclamation/compaction of append-only nonce, retirement, source-set, and checkpoint metadata;
- physical power-loss/filesystem qualification of the sync ordering;
- a stronger mechanism closing the Linux same-UID final identity-check -> unlink race;
- production key provisioning/rotation or a non-rollbackable freshness anchor;
- filesystem free-space reservation against unrelated concurrent consumption;
- confidentiality of clear sorter object IDs or spill geometry;
- production qualification outside native Linux x86_64 crypto evidence;
- a stable or accepted EXP-0003 wire format.

External deletion/replay rollback to an older valid authority set also remains outside HMAC integrity itself unless the caller supplies a trusted/non-rollbackable floor.

## Verification

Accepted implementation head before this evidence commit:

`c1acc2fa2fb7767dc0c8b4c4a81f1b4195410696`

Repository Rust workflow:

`31806947524` — fully green, including formatting, Clippy with warnings denied, full Rust implementation tests, Rust 1.85.0, i686, powerpc64, concrete HTTP/S3 assurance regressions, policy checks, independent-model/corpus checks, documentation, and framing replay.

The Phase 3 Evidence and immutable-successor vector workflows were also green on the accepted code head.

## Next consolidation slice

The next highest-value #11 slice is bounded crash-authoritative reclamation of append-only restart metadata.

The compaction design must establish a new authenticated nonce recovery base **before** deleting old nonce generations, prune Prepared retirement records only when matching Terminal authority exists, and prune source-set records only for restart generations already proven terminally retired. Active source binding and outstanding cleanup authority must survive compaction.

## Governance boundary

This remains private implementation/storage evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor public wire bytes, or make a compatibility promise.
