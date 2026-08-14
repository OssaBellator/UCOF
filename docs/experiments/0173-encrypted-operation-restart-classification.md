# Experiment 0173 — Encrypted operation restart classification

**Status:** non-normative Phase 3 encrypted-writer restart evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0157, 0171, and 0172

## Purpose

Experiment 0172 makes nonce authority crash-durable before any real AES-GCM session becomes issuable, but it only answers which nonce range is permanently burned after restart. It does not say whether an interrupted encrypted private stage still exists, whether it is the exact stage from the crashed operation, or whether it is safe to use as recovery input.

Experiment 0173 adds that classification boundary for the Experiment 0171 sorted encrypted descriptor spill stage.

The central rule is deliberately conservative:

> A durable pre-crash lease is never resumed for nonce issuance after restart. An exact surviving encrypted stage may be verified and reused only as read-only recovery input; any new encrypted output requires a fresh durable lease.

## Durable stage and manifest ordering

The long-lived nonce journal and encrypted working stage use separate pinned private directories.

The persistence order is:

`0172 durable lease -> build 0171 encrypted sorted spill -> copy to restart stage -> stage file sync_all -> stage directory sync_all -> HMAC stage manifest -> manifest file sync_all -> journal directory sync_all`

A durable manifest therefore never intentionally refers to a stage whose file and directory entry were left unsynchronized by the protocol.

An injected cut after stage directory sync but before the manifest proves the inverse rule: a durable stage without a durable manifest is not trusted or resumed.

## Manifest

The stage manifest is exactly 160 bytes:

- 128-byte canonical body;
- 32-byte HMAC-SHA256 tag using the Experiment 0172 journal authentication key.

It binds:

- manifest magic/version and stage role;
- derived AES key identity;
- nonce prefix;
- operation identity;
- nonce-journal generation;
- exact stage length;
- device and inode;
- SHA-256 of the encrypted stage bytes.

The current stage role is only `SortedDescriptorSpill`.

The manifest is stored in the durable journal directory; the stage itself is stored in the separate private stage directory.

## Strong filesystem identity

Experiment 0173 does not rely on inode alone.

The restart-stage identity is SHA-256 over a domain plus:

- device;
- inode;
- exact length;
- SHA-256 of the full encrypted stage contents.

Candidate stage files must also be:

- regular files;
- single-link files;
- owned by the effective UID;
- inaccessible to group/other permission bits.

The stage directory uses the same Linux descriptor-pinned/no-follow discipline as the preceding production-writer experiments.

Identity hashing is separately bounded by cumulative byte limits.

## Restart inventory classification

The scanner feeds the existing bounded generic restart classifier with the strong 32-byte stage identity rather than metadata-only identity.

The resulting dispositions are:

- `NoDurableManifestRestartWork` — no current durable manifest; a surviving unmanifested stage is not resume authority;
- `StageAbsentRestartWork` — complete readable scan proves the manifested stage is absent; the old lease stays burned and work restarts;
- `VerifiedExactNeedsFreshLease { object_count }` — exact manifested stage survives at the expected name and passes strong identity plus AEAD verification;
- `VerifiedRenamedNeedsFreshLease { object_count, actual_name }` — the exact stage survives under another name and passes the same verification;
- `RetainIndeterminate` — conflicting expected-name identity, truncated scan, unreadable identity, or insufficient classification evidence.

After inventory identifies an exact candidate, the candidate is reopened and its strong identity is rechecked before cryptographic verification. This closes the scan-to-verification identity seam within the experimental boundary.

## Cryptographic stage verification

Strong filesystem identity is not treated as a substitute for AEAD verification.

For the persisted Experiment 0171 spill stage, restart additionally requires:

- stage length is an exact multiple of the 100-byte encrypted spill payload;
- object count is within configured limits;
- the Experiment 0172 journal lease size is exactly `2 * object_count`, matching the current Experiment 0171 protocol;
- every spill nonce counter is unique and lies in the first half of the crashed lease;
- object ids are strictly increasing in sorted order;
- every AES-GCM record authenticates using the crashed operation id, crashed journal generation, key, prefix, object id, and counter;
- every decrypted 64-byte descriptor decodes successfully;
- decrypted descriptor object id matches the authenticated embedded object id;
- the file has exact end.

This is read-only cryptographic verification. It issues no nonce and does not reactivate the old lease.

A dedicated regression mutates ciphertext and then recomputes the in-memory file identity. The AEAD verifier still rejects it, proving that manifest/digest identity is not the sole integrity boundary.

## Bounded inventory limits

Default experimental limits are:

- 4096 directory entries;
- 4 MiB cumulative inventory metadata charging;
- 256 MiB cumulative identity hashing;
- 128 MiB maximum restart stage size;
- 1,000,000 maximum stage records.

If the expected file cannot be strongly identified within the identity budget, restart remains indeterminate rather than treating the file as absent.

## Executable restart cases

The accepted implementation proves:

1. **Exact durable stage** — verifies by strong identity and AEAD, reports the object count, preserves the old burned high-water, and a fresh lease starts strictly above the old `2 * object_count` range.
2. **Renamed durable stage** — rename plus directory sync is found by strong identity and classified as verified renamed input needing a fresh lease.
3. **Complete absence** — deletion plus directory sync becomes `StageAbsentRestartWork`; the crashed lease remains burned.
4. **Expected-name replacement** — moving the original aside and replacing its name with another file becomes `RetainIndeterminate`, even though the original exact identity exists elsewhere.
5. **Identity-budget exhaustion** — inability to hash the expected stage strongly becomes unreadable/indeterminate, never absence.
6. **Durable stage without durable manifest** — the stage is intentionally ignored as resume authority.
7. **Manifest tamper** — HMAC failure occurs before stage classification.
8. **Ciphertext mutation with recomputed file identity** — AEAD verification still fails.

## Fresh-lease requirement

If restart verifies a surviving encrypted spill, the crashed lease is still consumed permanently.

For an N-object Experiment 0171 spill:

- generation G reserved `2N` counters;
- the spill stage consumed the first N;
- restart treats all `2N` as burned because generation G was durably committed;
- verified stage bytes may be decrypted using generation-G context;
- any resumed retained-stage encryption must use a fresh generation G+1 lease.

This separation is the key correctness boundary for the next experiment.

## What this closes

Experiment 0173 establishes executable evidence that restart can distinguish:

- no durable stage authority;
- proven stage absence;
- exact surviving encrypted stage;
- renamed exact stage;
- conflicting or insufficient evidence;
- manifest tampering;
- encrypted-record tampering.

It does so without reactivating or reusing the crashed nonce lease.

## What remains open

Issue #11 remains open. Experiment 0173 does **not** yet:

- continue canonical emission from the verified old encrypted spill;
- split old-generation decryption context from fresh-generation retained-stage encryption in the existing transcode function;
- persist all preflight/output metadata needed to resume emission without redoing source metadata work;
- include the durable restart-stage copy and manifest in the existing end-to-end private-storage quota model;
- clean or retire verified/abandoned restart stages under crash-authoritative journal transitions;
- provide a production key/HMAC-key provisioning contract;
- provide a non-rollbackable freshness source;
- qualify power-loss behavior physically;
- encrypt locator/page-reference stages;
- conceal clear sorter object ids or spill geometry.

The current stage persistence also copies the final encrypted spill into a dedicated restart file. That extra overlap is intentional experimental evidence but is not yet priced into the production writer quota.

## Verification

Accepted implementation head before this evidence note:

`75316f449d264c536b8f5a7b5819f1fbd45cfc97`

Repository Rust workflow run:

`31800368001`

At documentation time this head had completed successfully on:

- locked dependency verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests including all Experiment 0173 restart regressions;
- Rust 1.85.0 and HTTP-feature MSRV;
- i686 portability;
- powerpc64 portability.

The broader repository replay continues through the usual HTTP/source, policy, documentation, independent parser/model, corpus, and framing checks.

## Next executable slice

The next slice is actual fresh-lease continuation from a verified old encrypted spill.

The existing Experiment 0171 transcode currently uses one `DescriptorEncryptionSession` for both:

- decrypting old spill records under their operation/generation context;
- issuing new retained-stage nonces.

That coupling must be split.

The correct continuation is:

`verify old stage -> construct read-only old spill context -> durably commit fresh lease -> decrypt old spill under old generation -> encrypt retained descriptors under fresh generation -> shared canonical emission`

The next experiment must prove that resumed public bytes and reports are exactly canonical while no nonce from the crashed generation is ever issued again.

## Governance boundary

This remains private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor wire bytes, or make a compatibility promise.
