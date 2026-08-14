# Experiment 0174 — Fresh-lease encrypted restart continuation

**Status:** non-normative Phase 3 encrypted-writer restart-continuation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0157, 0171, 0172, and 0173

## Purpose

Experiment 0173 proves that an exact surviving encrypted descriptor-spill stage can be identified and cryptographically verified after restart, while the crashed generation's durable nonce lease remains permanently burned.

Experiment 0174 completes the next correctness step: use that verified old encrypted spill as read-only recovery input, commit a fresh durable lease, re-encrypt the retained descriptor stage under the fresh generation, and feed the existing shared canonical emission loop.

The continuation is:

`verify old stage -> recover old read-only crypto context -> durably commit fresh N-counter lease -> decrypt old spill under generation G -> seal retained descriptors under generation G+1 -> existing canonical descriptor-reader emission`

No counter from the crashed generation becomes issuable again.

## Split old and fresh cryptographic authority

The key change is separating two responsibilities that Experiment 0171 originally coupled inside one session:

- **old read-only context**: AES key, nonce prefix, operation id, journal generation, and persisted lease range recovered from the crashed generation;
- **fresh write authority**: a new `DescriptorEncryptionSession` returned only after Experiment 0172 durably commits a later journal generation.

The old context is used only to authenticate/decrypt persisted sorter records. It has no nonce allocator.

The fresh session is used only to issue retained-stage nonces.

For N objects:

- crashed generation G had a `2N`-counter lease;
- its first N counters were used by encrypted sorter descriptors;
- all `2N` counters remain burned after restart;
- fresh generation G+1 receives exactly N counters, because sorting already completed before the crash;
- the N fresh counters seal the retained 92-byte descriptor stage.

## Bounded resumed transcode

The persisted old encrypted stage is streamed; no descriptor vector is reconstructed.

For every 100-byte old spill payload, continuation:

1. checks strict object-id order;
2. verifies the old nonce lies in the first half of the crashed `2N` lease;
3. rejects duplicate old nonce counters;
4. authenticates/decrypts under the crashed operation/generation context;
5. decodes the 64-byte `SourceDescriptor` and requires its object id to match;
6. derives the canonical output-size/source-buffer accounting from the authenticated descriptor;
7. allocates one counter from the fresh durable lease;
8. seals the descriptor into the 92-byte retained stage under the fresh operation/generation context.

At exact end, every expected old spill nonce must have been seen once and the fresh N-counter lease must be exhausted exactly.

The retained stage is then fully authenticated with the existing Experiment 0170 verifier before canonical emission.

## Recovered preflight accounting

Restart does not rerun the original plaintext descriptor sort merely to reconstruct emission metadata.

The facts needed by the shared canonical emission loop are derivable from authenticated descriptors:

- object count from exact persisted stage length;
- object record bytes from `OBJECT_HEADER_LEN + logical_len` for each descriptor;
- largest source buffer from `min(logical_len, max_source_read_bytes)`;
- page count/root level from the existing canonical tree-shape function;
- expected public bytes from header + objects + pages + snapshot + footer;
- preflight version-check count as one check per descriptor.

The original external sorter's internal run/merge geometry cannot be reconstructed from the final persisted spill. Experiment 0174 therefore creates an explicit restart provenance `BoundedSpillSortReport` with original run/merge counters set to zero and only the derivable final-stage facts populated:

- record count;
- persisted payload bytes;
- persisted stage SHA-256.

Those fields are private evidence only and do not affect public UCOF bytes.

## Typed continuation contract

The continuation API uses typed settings rather than a long positional argument list.

`EncryptedRestartContinuationSettings` groups:

- AES key;
- crashed generation;
- optional trusted rollback floor;
- restart inventory limits;
- source/output streaming options;
- immutable limits;
- fresh operation id.

`RestartTranscodeSettings` groups the source/output options and immutable limits used by the old-to-fresh transcode.

This keeps the crash/fresh authority boundary explicit without lint suppressions.

## Canonical exact-stage continuation

The primary executable case uses 17 objects.

Generation 1:

- durable old lease: counters 0..33 (`2N = 34` counters);
- persisted encrypted sorter spill verifies under generation-1 context.

Generation 2:

- fresh continuation lease: counters 34..50 (`N = 17` counters);
- retained descriptor stage is sealed entirely under generation 2.

The resumed writer then proves:

- exact byte equality with the normal canonical source writer;
- exact canonical source-writer report equality;
- crashed generation remains 1;
- fresh generation is 2;
- old and fresh nonce ranges are disjoint;
- recovered journal high-water is 51 after completion;
- the persisted old restart stage remains available;
- temporary retained/locator/page-reference working stages are retired.

## Renamed-stage continuation

A second executable case renames the strongly identified persisted encrypted stage and synchronizes its directory.

Experiment 0173 discovers it by strong identity. Experiment 0174 reopens that renamed stage, verifies it again, commits a fresh lease, and produces the same canonical public bytes/report.

The filename therefore does not become recovery authority; the manifest-bound strong identity remains authoritative.

## Failure before fresh lease allocation

If the encrypted stage was synchronized but its durable manifest was never committed, Experiment 0173 returns `NoDurableManifestRestartWork`.

Experiment 0174 then:

- returns before committing a fresh continuation lease;
- writes no public output;
- leaves the nonce journal at the crashed generation/high-water;
- leaves the unmanifested stage as non-authoritative private evidence.

This proves that merely finding encrypted bytes on disk is insufficient to advance nonce authority.

## Failure after fresh lease commit

A separate regression deliberately supplies source objects in the wrong caller order after a valid persisted spill has been verified.

The continuation has already:

- durably committed generation 2;
- consumed the fresh N-counter lease to seal the retained descriptor stage.

Canonical emission then detects that a descriptor's persisted `source_index` no longer refers to matching source metadata.

The result is intentionally fail-closed:

- no report is returned;
- generation 2 remains durable;
- its fresh nonce range remains burned;
- generation-1 counters are never reused or reissued;
- temporary working stages are retired.

The current shared emission loop writes the UCOF file header before performing the first per-source metadata comparison, so this specific late restart-source mismatch can leave a public-output prefix. Production atomic visibility still requires the existing private publication staging layer around the output writer.

## Source-set restart constraint

The persisted `SourceDescriptor` contains `source_index`, so a continuation source slice must preserve the original preflight source ordering/identity mapping.

Experiment 0174 validates the mapping during canonical emission, but it does not yet persist a separate source-set/order identity in the restart manifest.

A production restart contract should bind the external source set/order (or replace positional indices with a restart-stable source identity) before promising autonomous continuation across process reconstruction.

## What this closes

Experiment 0174 establishes executable evidence that:

- a verified old encrypted spill is usable as read-only restart input;
- the crashed `2N` nonce lease is never reactivated;
- resumed encryption uses a separately durably committed fresh generation;
- only N fresh counters are needed after the sorter phase has already completed;
- old-generation authentication and fresh-generation sealing remain cleanly separated;
- resumed exact and renamed paths preserve canonical public bytes and reports;
- restart emission metadata can be reconstructed without a plaintext re-sort;
- failures before fresh-lease allocation do not advance authority;
- failures after fresh-lease allocation burn only the newly committed range and remain fail-closed.

## What remains open

Issue #11 remains open. Experiment 0174 does **not** yet:

- durably mark the persisted old stage/manifest as consumed or retired after successful continuation;
- authorize/destructively clean the old stage by crash-consistent journal state;
- handle crashes between fresh continuation completion, public publication, old-stage cleanup preparation, unlink, directory sync, and terminal cleanup commit;
- include the persistent restart-stage copy, manifest, fresh retained-stage overlap, and journal files in one production private-storage quota;
- bind the external source set/order in the durable restart manifest;
- provide atomic public-output visibility by itself;
- provide a non-rollbackable freshness anchor;
- provide production key/HMAC-key provisioning;
- encrypt locator/page-reference stages;
- hide clear sorter object ids or spill geometry.

The reconstructed restart spill report also must not be misinterpreted as the original sorter's run/merge telemetry. Its zero run/merge fields explicitly mean "not reconstructed on restart," not "the original sort used no runs."

## Verification

Accepted implementation head before this evidence note:

`8c2f598ee1d12a29e2a2a074cacdb34861871074`

Repository Rust workflow run:

`31801131037`

At documentation time this head had completed successfully on:

- locked dependency verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests including the Experiment 0174 continuation regressions;
- Rust 1.85.0 and HTTP-feature MSRV;
- i686 portability;
- powerpc64 portability.

The broader repository replay continues through the usual HTTP/source, policy, documentation, independent parser/model, corpus, and framing checks.

## Next executable slice

The next slice is crash-authoritative retirement of the old restart stage and manifest.

A successful continuation should not simply unlink old private evidence ad hoc. The repository already has Experiments 0163–0166 for prepared cleanup ordering, strong restart identity, descriptor-pinned Linux observation, and crash-authoritative cleanup disposition.

The next experiment should reuse that ordering for this encrypted operation:

`successful continuation/publication evidence -> durable CleanupPrepared state naming exact stage+manifest identities -> unlink exact authorized objects -> stage/journal directory sync -> durable terminal cleanup generation`

Crash cuts must preserve the same rules:

- before durable cleanup preparation: no destructive cleanup authority;
- after preparation but before unlink: retry exact authorized cleanup;
- after unlink but before directory sync: do not claim terminal cleanup;
- after directory sync but before terminal commit: finalize without blindly deleting again;
- conflicting/renamed/tampered identity: retain indeterminate or resolve by strong identity, never delete by stale pathname.

The persistent stage copy and manifest should also enter the private-storage quota in that slice, because retirement semantics and storage qualification are now coupled.

## Governance boundary

This remains private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor wire bytes, or make a compatibility promise.
