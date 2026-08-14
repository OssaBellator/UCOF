# Phase 3 Consolidation Map

**Status:** working integration map; non-normative  
**Date:** 2026-08-14  
**Primary trackers:** #10, #11, #12, #13, #16, #76

## Purpose

Phase 3 now has enough executable evidence that the main risk is no longer missing isolated experiments. The risk is **evidence fragmentation**: normative convergence remains blocked on D1–D7 while issue #11 has accumulated two partially overlapping post-0170 implementation lines.

This document records what should be consolidated, what should remain separate, and which branch should be treated as the integration spine for the remaining writer/publication work.

It does not select EXP-0003 bytes or replace the Draft→Review ledger.

## Authority layers

Keep these layers distinct:

1. **`main`** — authoritative merged implementation/research baseline.
2. **FCP-0003 Draft + D1–D7 review ledger** — normative proposal layer; still decision-pending.
3. **open/stacked research branches** — executable evidence only.
4. **future selected EXP-0003 candidate corpus** — may be generated only after D1–D7 and the coordinated normative amendment.

A green research branch is not an accepted wire-format decision.

## Remote-source lane — #10

`main` already contains the complete provider-shaped strong-version assurance operation set from PRs #122–#129:

- concrete Reqwest/Tokio conditional range transport;
- explicit operation-wide retry/backoff authority;
- one-refresh application-owned authentication;
- native async cancellation/deadline handling;
- authenticated lookup/absence;
- strict full validation;
- linked-history validation;
- report-only recovery;
- S3-shaped immutable source adapter using opaque `versionId` and SigV4.

The remaining #10 work is qualification rather than architectural completion:

- live versioning-enabled S3 interoperability;
- IAM/version-specific permission behavior;
- STS/role refresh/expiry behavior;
- TLS trust-store / enterprise proxy policy;
- provider-scale request/byte/latency accounting.

Do not reopen already-landed async recovery or cloud-adapter implementation as missing work in Phase 3 status documents.

## Writer/publication lane — #11

### Common pre-divergence foundation

The post-#131 stack establishes:

- bounded deterministic external sorting;
- bounded source-metadata staging;
- bounded locator/page-ref tree staging;
- end-to-end canonical source genesis;
- operation-wide private-storage accounting;
- bounded private publication;
- private-stage crypto/restart/nonce contracts;
- descriptor-pinned Linux cleanup/restart evidence.

The important divergence occurs after the encrypted retained-descriptor integration point around commit `3578fa44562e7ca15d78a15215cb59750b6cfff4`.

### Confidentiality branch — PRs #135–#137

This line adds:

- encrypted sorter-run descriptor payloads;
- encrypted locator stages;
- encrypted page-reference stages at every tree level;
- exact tree nonce leases and encrypted-tree storage quotas;
- real HMAC-SHA256 for the earlier restart-journal abstraction.

The highest-value unique evidence in this line is now **encrypted tree staging**. The sorter and HMAC work overlap with later durability-line implementations.

### Durability/restart branch — Experiments 0171–0176

The newer integration spine adds:

- encrypted descriptor spill through the existing bounded sorter;
- a real AWS-LC authenticated durable nonce journal;
- descriptor-pinned durable encrypted restart-stage copies and manifests;
- restart classification by strong file identity;
- fresh-lease crash continuation;
- durable no-replace publication;
- crash-authoritative old-stage retirement;
- unified private-storage lifecycle accounting and pre-side-effect restart quota enforcement.

This line is further advanced in restart authority, durability sequencing, cleanup authority, and lifecycle integration.

Its remaining confidentiality gap is explicit: locator/page-reference private stages are still plaintext on this branch.

## Consolidation choice for #11

Use the **durability/restart line as the integration spine**.

Do not maintain two competing sorter-encryption or restart-authentication implementations indefinitely.

The consolidation target should be:

```text
bounded sorter
    -> encrypted descriptor spill
    -> durable encrypted restart checkpoint + authenticated manifest
    -> fresh-lease continuation when required
    -> retained encrypted descriptors
    -> encrypted locator/page-ref stages
    -> private canonical output staging
    -> durable no-replace publication
    -> crash-authoritative retirement
    -> one lifecycle storage cap
```

### What to transplant from the confidentiality line

Port the reusable encrypted tree-stage adapter and its writer/quota tests:

- fixed encrypted locator frames;
- fixed encrypted page-ref frames;
- stage-kind/ordinal/operation/generation/sequence/counter/width AAD binding;
- exact tree nonce count/lease requirement;
- ciphertext-tamper authentication failures;
- encrypted locator+leaf-ref and adjacent encrypted page-ref overlap accounting.

Adapt it to the durability line's existing `DescriptorEncryptionSession` and retained encrypted descriptor stage.

### What not to duplicate

Do not add a second durable sorter implementation merely to preserve branch history. The durability line already encrypts descriptor payloads before the generic bounded sorter and has restart-stage persistence built around that final encrypted spill.

Do not add a second restart HMAC format. The durability line already uses real HMAC-SHA256 for nonce journals, stage manifests, and retirement records with explicit durable generation authority.

Historical branch experiments should remain available as evidence even when their implementation path is superseded by the consolidated one.

## Immediate #11 sequence

1. Finish Experiment 0176 lifecycle quota CI and keep quota rejection before fresh nonce authority.
2. Port encrypted locator/page-ref staging from the confidentiality branch onto the durability spine.
3. Extend the unified lifecycle quota to use encrypted locator/page-ref widths and exact tree nonce leases.
4. Bind restart continuation to an authenticated durable external source-set/order manifest; object-count equality is not sufficient authority.
5. Design bounded journal/retirement/source-manifest compaction with crash-authoritative replacement.
6. Run physical/filesystem durability qualification and fault injection for the exact sync sequence claimed.
7. Decide the supported Linux isolation boundary or adopt a stronger mechanism for the remaining same-UID final-check -> unlink race.
8. Define production key provisioning/rotation/freshness-anchor policy without claiming more rollback resistance than the platform provides.

## Normative lane — #13, #16, #76

Implementation consolidation must not bypass the D1–D7 ballot.

The required order remains:

1. explicit maintainer dispositions D1–D7;
2. one coordinated normative amendment across FCP/spec/status/occupancy/disposition artifacts;
3. derive all geometry from selected bytes;
4. promote the recipe scaffold into a concrete candidate valid/invalid corpus;
5. reproduce the corpus in-repository;
6. move FCP-0003 Draft -> Review only after spec/corpus agreement;
7. obtain meaningful clean-room interpretation before experimental allocation;
8. migrate the reference implementation onto selected bytes.

The current research bytes remain evidence until that process occurs.

## Independent evidence lane — #12

Keep #12 separate from reference implementation volume.

Phase 3 still needs a meaningfully independent implementation or external clean-room review covering at least:

- exact-end bounded parser behavior;
- authenticated lookup/absence;
- linked history vs report-only recovery;
- one independently generated nontrivial file;
- invalid corpus comparison;
- resource/truncation evidence;
- mismatch classification without silently changing the independent result to match Rust.

## Exit criteria after consolidation

Phase 3 is not complete merely when the #11 branch is feature-rich.

Exit still requires all of the following to agree at the level claimed:

- selected experimental successor bytes/specification;
- candidate/authoritative corpus;
- reference implementation;
- independent interpretation/review;
- qualified remote-source behavior;
- qualified large-writer/publication behavior;
- explicit semantic/profile compaction boundary;
- continuous portability/fuzz/property/adversarial evidence;
- disposition of material FCP findings and rejected alternatives.

## Non-claims

This map does not:

- merge or close any existing PR;
- select D1–D7;
- allocate `UCOF-EXP-0003`;
- declare the current research branch production-ready;
- promise migration/compatibility;
- start Phase 4 byte dependencies.
