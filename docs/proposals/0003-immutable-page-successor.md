# FCP-0003 — Immutable-page directory successor

- **Status:** Draft
- **Authors:** UCOF maintainers
- **Created:** 2026-07-31
- **Rebased:** 2026-08-13 against consolidated `main` after PR #75
- **Supersedes:** the reusable-page direction of FCP-0002 Candidate 1
- **Proposed experimental epoch:** `UCOF-EXP-0003`, allocated only after an explicit maintainer decision
- **Tracking:** issues #13, #16, and #76

## Summary

This proposal defines the review target for a new disposable experimental epoch based on immutable content-addressed directory pages, complete append-only snapshots, exact-end active publication, explicit linked history, separately requested recovery, and bounded random-access validation.

It does **not** stabilize UCOF, allocate permanent registry identifiers, promise migration from earlier experiments, or make the current research microformat compatible with future revisions.

The proposed `UCOF-EXP-0003` marker remains unallocated while this FCP is Draft.

## Motivation

`UCOF-EXP-0002` Candidate 1 demonstrated bounded lookup, strict validation, append publication, linked history, recovery, repair, rewrite, and cross-language bytes. It also authenticated the active snapshot sequence inside every directory page. An unchanged historical page therefore could not be reused byte-for-byte: changing the sequence changed the page digest and every ancestor.

That is a wire-design blocker rather than a writer optimization problem.

The immutable-page successor removes active sequence from page identity. Existing pages are referenced by authenticated content digest and may be reused across snapshots. The consolidated implementation on `main` now provides substantial evidence for replacement, insertion, split propagation, deletion repair, root growth/collapse, canonical mixed batches, bounded source operation planning, history/recovery, rewrite, streaming output, transport policy, and staged publication research.

The remaining question is whether those experiments converge into one independently implementable experimental epoch.

## Scope

The proposed epoch includes:

- a fixed bootstrap header identifying one disposable epoch;
- complete immutable object records;
- fixed-size immutable authenticated B+tree pages;
- complete snapshot records and exact-end commit footers;
- append-only publication with a previous-footer link;
- strict active-state validation that never invokes recovery;
- authenticated lookup and absence;
- fully revalidated linked history;
- bounded suffix recovery that reports candidates without selecting one;
- verified-source rewrite;
- profile/application hooks for semantic compaction inputs;
- strong-version source-view requirements for mutable remote objects;
- opaque capability/extension mechanisms needed to test evolution behavior.

Transforms, compression, schemas, signatures, provenance, encryption, selective disclosure, external references, and stable domain profiles remain outside this experimental epoch except where opaque declarations are necessary to exercise compatibility behavior.

## Architectural boundary

UCOF is a **universal container, not a universal representation**.

This FCP defines a small structural core. Application semantics, semantic dependency interpretation, transforms, schemas, cryptographic services, and domain-specific indexes belong in optional services or profiles rather than becoming mandatory core behavior.

## Proposed wire-policy package

Acceptance for experimentation requires one self-contained `spec/experimental/UCOF-EXP-0003.md` containing the complete field table and algorithms. Existing research files are evidence, not normative by reference.

### Identifiers

- Primary object identifiers are proposed as **128-bit opaque unsigned values**.
- Canonical key ordering is lexicographic over the 16-byte big-endian key representation.
- Zero is reserved as absent and is not a valid object identifier.
- Identifiers are lookup keys, not content digests, authenticity claims, or globally collision-free names.
- Applications choosing random identifiers remain responsible for adequate collision policy.

Rationale: identifier longevity and namespace independence should be decided separately from locator density. The current 64-bit research layout is useful implementation evidence but is not automatically the proposed epoch layout.

### Primary locator

The primary leaf entry is proposed as a fixed **64-byte minimal authenticated locator** containing:

- 16-byte object identifier;
- 8-byte record offset;
- 8-byte record length;
- 32-byte object digest.

Object kind and logical length remain authenticated inside the object header but are not mirrored in the primary locator.

Profiles needing broad inventory without per-object header reads may define an authenticated optional secondary inventory index. No permanent reserved tail is included in every primary locator.

### Page geometry

- Page size: **16 KiB** for this disposable epoch unless review evidence justifies a change.
- Page identity: domain-separated digest over complete canonical page bytes.
- Page identity excludes active snapshot sequence, physical offset, and file-instance commit identity.
- Leaf and internal entry layouts are fixed for the epoch.
- Unknown page kinds fail closed.
- All unused page bytes are zero and participate in page identity.

The exact internal child-reference field table must be specified explicitly in the EXP-0003 document before this FCP moves to Review.

### Occupancy

The proposed occupancy policy is specified by `docs/spec/IMMUTABLE_SUCCESSOR_OCCUPANCY_POLICY.md`.

In summary:

- non-root pages contain at least half maximum capacity, rounded up;
- a root leaf may contain one or more entries;
- an internal root contains at least two children;
- the active tree is non-empty;
- canonical full construction packs left-to-right, redistributing only the final two pages when required to satisfy minimum occupancy;
- the same policy applies independently at leaf and internal levels.

Exact capacities are derived from the accepted EXP-0003 entry layouts rather than copied from the current 64-bit/88-byte research geometry.

### Deterministic insertion

- Route by inclusive child key ranges.
- Replacing an existing identifier produces new immutable bytes along the affected path.
- Insert a missing identifier into the target leaf in canonical key order.
- On overflow, split deterministically so both pages satisfy the occupancy policy.
- Propagate replacement/new-right-child information upward.
- Apply the same rule to internal overflow.
- Create a new root when the previous root splits.

The exact split formula is byte-significant and must agree with the occupancy companion and authoritative vectors.

### Deterministic deletion

- Delete only an existing identifier; deleting an absent identifier is an error.
- Reject deletion of the final active object unless this proposal is revised to define an empty-tree representation.
- If a non-root page underflows, borrow from the left sibling first when that sibling can remain conforming.
- Otherwise borrow from the right sibling.
- Otherwise merge with the left sibling when present; if no left sibling exists, merge with the right.
- Recompute parent ranges after redistribution or merge.
- Repair internal underflow recursively.
- Collapse an internal root with one child to that child.

Sibling preference and merge direction are byte-significant.

### Batch semantics

- A batch contains at most one operation per identifier.
- Duplicate operation identifiers are rejected.
- Caller operation order does not affect canonical bytes.
- The writer canonicalizes operations by identifier and computes one complete next snapshot.
- Multiple updates sharing ancestors emit each changed page once for the selected canonical algorithm.
- Publication occurs only after all object/page/snapshot bytes needed by the commit are complete.

### Canonical final-state question

Review must explicitly decide whether two valid construction paths that produce the same active logical object set are required to produce the same canonical page partition and root identity.

The current research canonical mixed/full-construction path aims for this property. Persistent path-local mutation can preserve additional historical page identity. EXP-0003 must state which identity guarantees are normative rather than leaving this as an implementation accident.

## Assurance boundaries

The following claims remain distinct:

### Strict active validity

The exact-end current commit and all objects/pages required by the active snapshot validate under caller-controlled limits.

Strict validation never invokes recovery.

### Targeted lookup

The active commit, one authenticated directory path, and the selected object or authenticated absence validate. Unrelated payload objects are not thereby claimed valid.

### Verified history

Every linked historical prefix selected by the history operation is strictly revalidated under cumulative bounds.

### Recovery evidence

Bounded scanning may report exact strictly valid prefix candidates. Recovery never silently selects a candidate as the active state.

### Stable source view

One remote assurance operation is bound to one strong non-ABA source-version token. Stable source view prevents mixed-version acceptance but does not establish freshness.

### Freshness

Freshness/rollback resistance requires trusted external state and application authorization policy. File integrity alone cannot authorize initial pinning or a later checkpoint advance.

### Semantic compaction

The core does not infer arbitrary dependencies from opaque payloads. Semantic compaction requires a profile/application dependency resolver plus explicit behavior for unknown semantics.

### Rewrite identity

Repair, rewrite, and semantic compaction create new byte/commit identity unless a narrower explicitly documented identity is preserved. Byte-scoped signatures must not be reported as surviving changed bytes.

## Compatibility impact

This proposal is intentionally incompatible with `UCOF-EXP-0001` and `UCOF-EXP-0002`.

Readers must reject unknown epochs rather than infer a layout.

No migration support is promised for disposable experimental files. If accepted, later incompatible byte changes require another experimental epoch.

Acceptance of FCP-0003 for experimentation still does not create a UCOF 1.0 compatibility commitment.

## Security requirements

A conforming experimental implementation must:

- use checked arithmetic for every range/count calculation;
- reject object/object, page/page, and object/structural overlap;
- bound source reads, requests, hashed bytes, allocations, object/page counts, depth, history work, recovery attempts, and diagnostics;
- avoid allocating directly from untrusted declared lengths without policy checks;
- keep exact-end validation separate from recovery;
- validate page and object identities in unambiguous domain-separated scopes;
- reject unknown required capabilities while preserving integrity evidence where possible;
- preserve unknown optional extension bytes only under an explicit rewrite policy where preservation is promised;
- distinguish integrity from authenticity, confidentiality, provenance, signer trust, authorization, and freshness;
- never claim byte-scoped signatures survive changed bytes;
- fail closed on ambiguous structural or capability interpretation.

## Draft → Review gates

This proposal may move from Draft to Review when the proposal is sufficiently precise for independent criticism, not only after all production qualification is complete.

Required before Review:

1. complete the self-contained EXP-0003 field table and binary grammar;
2. resolve identifier, locator, internal-reference, occupancy, split, deletion, empty-tree, batch, identity, capability, and canonical-final-state questions;
3. integrate the exact occupancy companion into the normative package;
4. transfer every material FCP-0002 objection into FCP-0003, a rejected alternative, or an explicitly owned later phase;
5. produce authoritative structural boundary vectors for genesis, replacement/reuse, split/root growth, deletion/borrow/merge/root collapse, mixed batch, history, and recovery;
6. document mismatches between current research bytes and proposed EXP-0003 bytes;
7. commit a maintainer-review-ready Candidate 1 disposition record.

The consolidated Rust implementation already supplies substantial evidence for item 5, but the authoritative vectors must be regenerated from the accepted EXP-0003 layout rather than inheriting research identities.

## Review → Accepted-for-experimentation gates

Before allocating `UCOF-EXP-0003`, require:

1. resolution of material Review objections;
2. one complete experimental specification with no undefined byte-significant ranges or host-language behavior;
3. Rust generation/validation of the authoritative valid and invalid corpus;
4. at least an initial clean-room independent interpretation of the specification;
5. explicit Candidate 1/FCP-0002 disposition;
6. documented security/threat-model delta for the new epoch;
7. explicit statement of remaining production and interoperability non-claims.

Full independent implementation, maintained remote adapters, and production publication qualification remain **Phase 3 exit** gates under #12, #10, and #11 respectively; they need not prevent the proposal from entering Review.

## Phase 3 exit evidence

The EXP-0003 interoperability candidate is not complete until:

- an independent implementation or external clean-room review satisfies #12;
- real HTTP plus one immutable-version cloud-object source is qualified under #10;
- production-candidate spill/publication behavior is qualified under #11;
- semantic compaction/profile dependency rules converge;
- authoritative cross-implementation valid/invalid vectors agree;
- continuous fuzz/property/portability/adversarial evidence remains green;
- all material FCP findings are dispositioned.

## Alternatives considered

### Keep Candidate 1 and optimize the writer

Rejected. Active sequence is part of Candidate 1 page identity, so exact historical page reuse is impossible without changing page bytes and digests.

### Use 64-bit identifiers to minimize entries

Not selected for this proposal. The density benefit is real, but a universal-container experiment benefits from larger opaque namespace headroom. Review may reverse this choice with explicit workload, collision-policy, and scale evidence.

### Mirror kind and logical length in every primary locator

Deferred to optional authenticated inventory structures. Mirroring reduces header requests during broad inventory but permanently increases every primary locator.

### Variable-length primary entries

Rejected for this epoch. Offset tables, canonical packing, and parser-differential risk add complexity before the fixed-entry design has independent reproduction.

### Make recovery part of ordinary open/validation

Rejected. Recovery evidence is weaker and more ambiguous than exact-end validity and must remain explicitly requested.

### Treat a stable remote version as freshness

Rejected. A stable source token prevents mixed-version reads but cannot prove that the object is the newest authorized state.

## Open objections requested

Review should focus especially on:

- 128-bit identifier cost/order;
- minimal locator density versus broad inventory I/O;
- exact internal child-reference layout;
- 16 KiB page-size trade-offs;
- occupancy and split compatibility;
- deterministic deletion sibling preference;
- empty-tree policy;
- canonical final-state identity versus historical page reuse;
- batch duplicate/canonicalization semantics;
- catalog/capability/extension placement;
- source-version and freshness boundaries;
- rewrite/compaction preservation rules;
- when independent implementation evidence is mandatory relative to Review and epoch allocation.
