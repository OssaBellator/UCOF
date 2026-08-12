# FCP-0002 → FCP-0003 Objection Transfer

**Status:** Draft transfer record for maintainer review  
**Date:** 2026-08-13  
**Source:** FCP-0002 Candidate 1 and its Phase 3 evidence  
**Target:** FCP-0003 Draft / proposed `UCOF-EXP-0003`  
**Tracking:** issues #13, #16, #76

## Purpose

FCP-0002 Candidate 1 produced valuable positive and negative evidence, but it must not remain a competing reusable-page promotion baseline after the immutable-page successor is selected for review.

This document classifies the material Candidate 1 blockers and design objections so that none are lost when Candidate 1 is dispositioned.

Each item is classified as one of:

- **Resolved by successor policy** — FCP-0003/EXP-0003 contains an explicit replacement direction and the old Candidate 1 choice is rejected;
- **Open EXP-0003 Review decision** — the successor has a concrete proposal but the byte-significant choice still requires review;
- **Phase 3 implementation qualification** — important for Phase 3 exit/production claims but not a missing wire-format rule;
- **Profile/later-phase responsibility** — intentionally outside the universal structural core;
- **Retained rejected-alternative/security evidence** — Candidate 1 remains useful for regression and rationale.

This transfer record does not itself accept FCP-0003 or allocate `UCOF-EXP-0003`.

---

## Executive disposition

Candidate 1 should be treated as:

> **Superseded as the reusable-page Phase 3 baseline; retained as disposable negative, security, interoperability, and regression evidence.**

The decisive reason is structural: Candidate 1 authenticates active snapshot sequence inside every page, making exact historical page reuse impossible without changing page bytes/digests and all ancestors.

The immutable-page successor removes active sequence from page identity and therefore addresses the architectural blocker rather than attempting to optimize around it.

---

## Transfer of FCP-0002 “Remaining blockers before Review”

### 1. Page sequence prevents exact historical page reuse

**FCP-0002 blocker:** replace Candidate 1 page-sequence semantics with immutable-page or page-birth semantics, then demonstrate deterministic batched byte-level reuse.

**Classification:** **Resolved by successor policy; implementation evidence exists.**

**Successor disposition:**

- FCP-0003 proposes immutable content-addressed pages;
- active sequence, physical offset, and file-instance commit identity are excluded from page identity;
- consolidated `main` demonstrates exact page reuse across replacement/insertion/deletion/mixed operations;
- persistent/source-backed mutation and page-accounting evidence is already present.

**Still required:** authoritative EXP-0003 vectors must be regenerated from accepted EXP-0003 field widths. Existing research identities are evidence, not final vectors.

**Trackers:** #13, #16, #76.

---

### 2. Leaf layout and object identifier width

**FCP-0002 blocker:** select or replace the 88-byte leaf layout and decide identifier width.

**Classification:** **Open EXP-0003 Review decision with a concrete proposal.**

**Successor proposal:**

- 16-byte opaque `ObjectId`;
- 64-byte minimal primary locator: 16-byte ID, 8-byte record offset, 8-byte record length, 32-byte object digest;
- no mirrored kind/logical-length fields in every primary locator;
- no permanent 16-byte reserve tail;
- broad inventory acceleration belongs in an optional authenticated secondary index/profile service.

The first EXP-0003 Draft also proposes a 64-byte object header carrying the 16-byte ID.

**Evidence retained:** Candidate 1 scale comparisons show the 88-byte/u64 layout is larger than a 64-byte/128-bit minimal locator at 100 million objects, while mirrored metadata can reduce object-header reads. That trade-off remains relevant Review evidence.

**Trackers:** #13, #76.

---

### 3. Bounded external sort integration, spill cleanup/confidentiality/descriptors/storage exhaustion

**FCP-0002 blocker:** integrate bounded external sorting with page emission and define production spill policy.

**Classification:** split into **resolved research evidence** plus **Phase 3 implementation qualification**.

**Already demonstrated on `main`:**

- bounded external sorting;
- descriptor-limited multi-pass merge;
- deterministic page/output behavior across run sizes/fan-in;
- private staging/no-overwrite publication research;
- ownership-token cleanup models;
- Unix staging/publication, fault injection, restart classification, and directory pinning.

**Still open under #11:**

- encrypted spill when required;
- descriptor-relative hardening;
- authenticated durable restart journal;
- platform/filesystem-specific durability qualification;
- physical power-loss evidence where claimed;
- production cleanup/quarantine semantics.

**Wire-format disposition:** these are storage/writer implementation requirements, not missing EXP-0003 byte fields, except that the wire format must keep incomplete publication distinct from exact-end validity.

**Trackers:** #11, #76.

---

### 4. Normative minimum resource limits versus caller policy

**FCP-0002 blocker:** decide universal numeric minima versus caller policy.

**Classification:** **Resolved policy direction; exact documentation still needs Review wording.**

**Successor position:**

- structural limits derived from the epoch layout are normative (field widths, page capacity, occupancy, exact lengths);
- implementations must provide caller-controlled work/resource limits;
- no universal minimum RAM/read/request budget is proposed as a condition of byte validity;
- a resource-policy refusal is distinct from a malformed-file determination;
- implementations must not allocate or perform unsafe work solely from untrusted declared lengths.

This keeps security limits part of conformance without pretending every conforming implementation must accept the same maximum file/object count in one operation.

**Required Review wording:** define which operations/conformance claims require which categories of configurable limit and how `resource limit exceeded` differs from malformed/unsupported.

**Tracker:** #76.

---

### 5. Future-field and capability preservation rules

**FCP-0002 blocker:** define future-field and capability-preservation behavior.

**Classification:** **Partially resolved; remaining EXP-0003 Review decision.**

**Successor semantics already proposed:**

- unknown required capability blocks safe interpretation that requires it;
- unknown optional capability may be skipped where safe;
- structural/integrity evidence may remain reportable despite unsupported interpretation;
- rewrite APIs that promise preservation must preserve unknown optional extension bytes exactly or reject the rewrite;
- unknown required data must never be silently discarded during rewrite/compaction.

**Still unresolved before Review:** exact authenticated encoding/location of catalog roots, required/optional capabilities, and extension records—or an explicit decision to narrow those bytes out of EXP-0003 scope.

**Tracker:** #13 / #76.

---

### 6. Profile-level history retention and semantic compaction inputs

**FCP-0002 blocker:** define history retention and semantic compaction inputs.

**Classification:** **Core boundary resolved; semantic rules are profile/later-phase responsibility.**

**Successor core position:**

- strict active validity, linked-history validity, and recovery are distinct core assurance modes;
- rewrite/compaction creates new byte/commit identity when bytes change;
- the core understands structural reachability only;
- arbitrary payload dependency semantics are supplied by a profile/application resolver;
- unknown dependency semantics must fail closed or trigger an explicit conservative retention policy;
- byte-scoped signatures/provenance cannot be falsely reported as preserved through changed bytes.

**Still required for Phase 3 exit:** converge the existing semantic/profile research into one explicit interface plus at least one authoritative profile-defined compaction vector.

**Longer term:** Archive/Table and other profiles define domain semantics in Phase 8 rather than bloating the mandatory core.

**Tracker:** #76 plus remaining semantic/profile PRs/issues.

---

### 7. Transport retry, cancellation, deadline, async coalescing under stable-view requirements

**FCP-0002 blocker:** define transport behavior without weakening one-version source assurance.

**Classification:** **Core assurance boundary resolved; maintained adapters remain Phase 3 implementation qualification.**

**Successor/core policy already established:**

- one assurance operation binds to one strong non-ABA source version;
- version change terminates the operation cleanly;
- retry budgets, delays, HTTP classification, cooperative waits, authentication refresh, cancellation/deadline outcomes remain distinct;
- stable source view does not establish freshness.

**Still open under #10:**

- maintained real HTTP adapter;
- one versioned cloud-object adapter;
- native asynchronous cancellation of in-flight requests;
- provider/TLS/redirect/cache/decompression qualification.

**Wire-format disposition:** transport implementation is not encoded into EXP-0003 bytes.

**Trackers:** #10, #76.

---

### 8. Independent implementation/external review

**FCP-0002 blocker:** obtain independent implementation/review outside the repository.

**Classification:** **Still required; moved to explicit interoperability gate.**

In-repository Rust/Python differential evidence remains useful but can share a specification misunderstanding.

FCP-0003 now separates gates:

- initial clean-room interpretation is required before experimental epoch allocation;
- independently maintained implementation or reproducible external clean-room review remains a hard Phase 3 exit gate.

Mismatch handling must classify spec/reference/independent/vector defects before changing implementations merely to agree.

**Tracker:** #12 / #76.

---

### 9. External trusted freshness / rollback resistance

**FCP-0002 blocker:** define freshness without conflating it with integrity or stable source view.

**Classification:** **Resolved boundary; application authorization remains external.**

The successor guidance distinguishes:

- unpinned integrity;
- current trusted checkpoint;
- authorized advance candidate;
- rollback;
- same-sequence fork;
- unrelated/unverifiable advance.

Initial pinning and checkpoint advancement require application authority. File integrity or a stable source token cannot silently create/advance trust.

The format may provide authenticated sequence/snapshot/commit/ancestry evidence, but applications define authorization, quorum, transparency, or conflict-resolution policy.

**Reference:** `docs/security/FRESHNESS_CHECKPOINT_AUTHORIZATION.md`.

**Tracker:** #76; later signature/trust work in Phase 6.

---

### 10. Resolve substantive maintainer objections and proposal review period

**FCP-0002 blocker:** complete maintainer review/disposition.

**Classification:** **Still open normative governance decision.**

Candidate 1 cannot be formally superseded merely because the reference implementation is green.

The maintainer decision should record:

- Candidate 1 disposition;
- FCP-0002 status;
- FCP-0003 status;
- whether/when `UCOF-EXP-0003` is allocated;
- material objection transfer completion;
- required follow-up.

**Tracker:** #13.

---

## Additional Candidate 1 design objections and alternatives

### Page size: 4 KiB vs 16 KiB vs 64 KiB

**Classification:** **Open EXP-0003 Review decision with current proposal = 16 KiB.**

Candidate 1 measurements showed the expected trade-off: smaller pages reduce authenticated lookup-path bytes but increase depth/metadata overhead; larger pages reduce depth but increase targeted path transfer.

FCP-0003 retains 16 KiB as a provisional midpoint. The first EXP-0003 Draft derives 254 leaf locators and 226 child references from that size and its proposed headers.

Before acceptance, keep at least one scale/range-I/O comparison using the new 64/72-byte entries rather than relying only on Candidate 1 geometry.

---

### Mirrored kind/logical length in every primary locator

**Classification:** **Proposed rejected/deferred alternative.**

Candidate 1 mirrors metadata useful for broad inventory but permanently enlarges every primary entry.

FCP-0003 proposes a minimal locator and optional authenticated inventory index for profiles/workloads needing broad metadata enumeration without per-object header reads.

This remains a Review trade-off and should receive explicit workload evidence.

---

### Reserved bytes in every leaf entry

**Classification:** **Rejected for EXP-0003 primary locator.**

Candidate 1’s 16-byte locator tail costs substantial metadata at large object counts without an assigned semantic use.

The EXP-0003 Draft proposes no per-locator reserve. Extension should occur through explicitly versioned/optional structures rather than permanent tax in every primary entry.

---

### Variable-length primary entries

**Classification:** **Rejected for EXP-0003 Draft.**

Variable entries introduce offset-table/canonical packing/parser-differential complexity before fixed-entry interoperability has been independently reproduced.

A later epoch may revisit with evidence.

---

### Ordered B+tree versus monolithic sorted array/hash page layouts

**Classification:** **Resolved proposal direction; alternatives retained as experimental evidence.**

EXP-0003 selects an ordered immutable B+tree because it supports authenticated range routing, absence, deterministic mutation, and localized page reuse.

Monolithic sorted arrays remain useful comparison evidence but rewrite broadly. Hash-oriented layouts may offer different lookup/update trade-offs but are not selected for this epoch.

---

### Complete checkpoints versus weaker progress markers

**Classification:** **Resolved for EXP-0003 core.**

Active snapshots/commit footers represent complete independently valid states. A weaker progress marker, if later needed, must not be confused with exact-end active validity and should use a separate explicitly weaker structure/capability.

Checkpoint cadence remains application/writer policy.

---

### Strict validation versus automatic recovery

**Classification:** **Resolved and retained as a core invariant.**

Strict validation requires one exact-end footer and never searches backward. Recovery is explicit, independently bounded, report-only, and validates every returned prefix strictly.

This separation is security-critical and carries unchanged into FCP-0003.

---

### Structural snapshot identity versus file-instance commit identity

**Classification:** **Resolved and retained.**

The successor keeps structural snapshot/page/object identities distinct from one file-instance append commit identity. Repair/rewrite may preserve narrower content/state facts while creating new commit identity.

No byte-scoped signature-preservation claim follows from semantic equivalence.

---

### Global canonical final-state root versus historical page reuse

**Classification:** **New explicit EXP-0003 Review decision exposed by successor evidence.**

The first EXP-0003 Draft proposes **scoped determinism**:

- canonical fresh genesis/rewrite has a deterministic bulk grouping;
- persistent append is deterministic from prior tree + canonicalized operations;
- persistent append may preserve historical pages and therefore produce a history-sensitive root differing from fresh rewrite of the same logical active set.

This avoids sacrificing copy-on-write scale merely to force one structural root for every logically equal state.

If Review rejects this position, the alternative must explain the expected rewrite amplification and identity semantics.

---

## FCP-0002 status recommendation after transfer

Once this transfer record is reviewed and issue #13 records the maintainer decision, update FCP-0002 to state prominently:

> Candidate 1 is superseded as the reusable-page baseline. Its implementation, vectors, experiments, and security findings remain historical disposable evidence. The successor interoperability direction is FCP-0003 / proposed EXP-0003. No Candidate 1 migration or compatibility promise exists.

Do not delete or silently reinterpret Candidate 1 artifacts.

---

## Items that still block FCP-0003 moving from Draft to Review

This transfer narrows the true normative blockers to a smaller set:

1. approve/revise the proposed 128-bit ID, 64-byte locator, 64-byte object header, 80-byte page header, 72-byte child reference, 16 KiB page size, and resulting capacities;
2. approve/revise half-full occupancy, split/delete/empty-tree rules;
3. approve/revise scoped determinism/history-sensitive persistent root identity;
4. define exact catalog/root/capability/extension encoding or explicitly remove it from EXP-0003 scope;
5. settle SHA-256/domain/algorithm-rigidity and experimental object-kind namespace rules;
6. publish authoritative structural boundary vectors from the accepted Draft layout;
7. record the Candidate 1/FCP-0002 maintainer disposition.

Maintained HTTP/cloud adapters, full independent implementation, and production publication qualification remain Phase 3 exit requirements but should not block beginning FCP-0003 Review once the wire package is precise.

---

## Decision record

```text
Transfer reviewed: yes | no
Date:
Reviewer/maintainer:
Candidate 1 reusable-page disposition:
FCP-0002 status after transfer:
Items retained as rejected alternatives:
Items transferred to FCP-0003 Review:
Items transferred to Phase 3 implementation gates:
Items transferred to profiles/later phases:
Blocking corrections to this transfer record:
```
