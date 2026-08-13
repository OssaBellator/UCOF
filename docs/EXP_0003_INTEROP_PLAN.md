# EXP-0003 Interoperability Convergence Plan

**Status:** Active P0 convergence; maintainer dispositions pending  
**Started:** 2026-08-13  
**Implementation baseline:** consolidated `main` after PR #75  
**Primary tracker:** #76  
**Current Draft→Review ledger:** `docs/review/FCP_0003_DRAFT_TO_REVIEW_LEDGER.md`

## Goal

Turn the consolidated Phase 3 research implementation into one independently implementable **disposable experimental successor epoch** before Phase 4 transform/compression work establishes new byte dependencies.

The remaining risk is specification drift rather than lack of implementation. Research code, first-Draft byte tables, historical Candidate 1 artifacts, and later evidence must converge through explicit maintainer decisions rather than implementation inertia.

UCOF remains a **universal container, not a universal representation**. Core should define a compact safe envelope; transforms, schemas, signatures, encryption, storage adapters, and domain semantics belong in services/profiles unless a concrete Core dependency is proven.

## Current sequencing rule

Do not:

- treat merged review packets as accepted policy;
- change FCP-0003 bytes piecemeal while the D1–D7 ballot is unresolved;
- regenerate “authoritative EXP-0003” identities against decision-pending bytes;
- allocate `UCOF-EXP-0003` merely because FCP-0003 enters Review;
- start Phase 4 in a way that depends on unresolved successor bytes.

Do:

1. disposition the bounded maintainer ballot;
2. apply all selected choices in one coordinated normative amendment;
3. generate a new candidate interoperability corpus from those exact bytes;
4. reproduce it in-repository;
5. move FCP-0003 Draft→Review only when the Review gate is satisfied;
6. obtain meaningful clean-room interpretation before experimental allocation;
7. migrate the reference implementation to the selected bytes;
8. continue the broader Phase 3 qualification/independence gates.

# P0 — Maintainer decision ledger

PR #117 reduced the current byte-significant policy package to seven explicit choices. All remain **unselected** until a maintainer records a disposition.

## D1 — Candidate 1 / FCP-0002 disposition

Primary tracker: #13.

Recommended:

> `UCOF-EXP-0002` Candidate 1 is superseded as the reusable-page baseline and retained as historical negative/security/interoperability/regression evidence, with no migration or compatibility promise.

Decision evidence:

- `docs/PHASE_3_DISPOSITION_DRAFT.md`
- `docs/review/FCP_0002_TO_0003_OBJECTION_TRANSFER.md`

The formal disposition remains pending.

## D2 — ObjectId and primary-directory geometry

Recommended Review candidate:

```text
ObjectId                  8 opaque bytes
ordering                  unsigned lexicographic byte order
scope                     container-context structural lookup key
Core no-remap merge       no generic guarantee
page size                 16,384 bytes
object header             40 bytes
page header               40 bytes
leaf locator              56 bytes
internal reference        56 bytes, explicit child min + max
leaf/internal C           291
leaf/internal M           146
leaf/internal overflow    292 -> 146,146
```

Tight 128-bit remains the explicit alternative if Core intentionally adopts a stronger uncoordinated/no-remap identifier contract.

Evidence:

- Experiments 0010, 0107–0109, 0135–0137
- `docs/review/EXP_0003_IDENTIFIER_GEOMETRY_DECISION_PACKET.md`

## D3 — occupancy and split policy

Primary tracker: #16.

Recommended:

- half-full non-root occupancy;
- deterministic final-two-group redistribution;
- explicit root exceptions;
- deterministic overflow split derived from selected capacity.

If tight-64 is selected, both page kinds use `C=291`, `M=146`, overflow `292 -> 146,146`.

## D4 — deletion borrower policy

Recommended:

```text
if both siblings can lend:
    choose fuller sibling
    exact occupancy tie -> left
otherwise:
    use the one eligible sibling
merge fallback:
    retain deterministic existing order
```

Current Rust research bytes remain LeftFirst until maintainer disposition.

Evidence:

- Experiments 0110–0134
- `docs/review/EXP_0003_DELETE_POLICY_DECISION_PACKET.md`

## D5 — catalog/root/capability/extension binding

Current review proposal:

- `docs/review/EXP_0003_CATALOG_CAPABILITY_PROPOSAL_V2.md`

Recommended:

- every snapshot selects one ordinary authenticated catalog object;
- linked child snapshots preserve one stable catalog structural slot;
- catalog changes replace bytes under that same ObjectId;
- Core permits `root_count == 0`;
- mandatory catalog keeps the physical primary tree non-empty while application roots may be empty;
- capability records carry required-support semantics;
- extension records are sorted opaque length-delimited metadata with explicit preservation rules.

Until D2 is selected, the proposal remains width-parametric:

```text
snapshot length = 96 + ObjectId width
```

which is 104 bytes for the recommended 8-byte key or 112 for the tight-128 alternative.

## D6 — hash/domain/magic/kind package

Current packet:

- `docs/review/EXP_0003_HASH_MAGIC_KIND_DECISION_PACKET.md`

Recommended:

- SHA-256 only for this disposable epoch;
- exact current object/page/snapshot/commit domain strings;
- exact current structural magics;
- no algorithm-agility field;
- page kinds `1=leaf`, `2=internal`, all others invalid;
- object kind `0` invalid;
- object kind `1` catalog if D5 is accepted, otherwise Core-reserved;
- object kinds `2..65535` structurally opaque application/profile tags.

## D7 — scoped determinism

Current packet:

- `docs/review/EXP_0003_SCOPED_DETERMINISM_DECISION_PACKET.md`

Recommended:

- fresh canonical rewrite defines normalized current-state structural form;
- persistent mutation is deterministic from exact prior validated bytes + canonicalized semantic batch;
- persistent mode may reuse old records/pages at their existing authenticated offsets;
- equal logical active states reached through different histories need not have equal root/snapshot digests.

Experiment 0138 corrects the rationale: history-independent dynamic partitioning can satisfy half-full/hard-maximum B-tree occupancy, but canonical partitioning alone cannot remove the current format's authenticated physical object/page/root offsets. A history-independent persistent root would require a broader placement/reference/randomness design.

# P0 — Coordinated normative amendment

After D1–D7 are dispositioned, make **one** normative change set so the selected policy appears consistently in:

1. `docs/proposals/0003-immutable-page-successor.md`
2. `spec/experimental/UCOF-EXP-0003.md`
3. `docs/spec/IMMUTABLE_SUCCESSOR_OCCUPANCY_POLICY.md`
4. Candidate 1/FCP-0002 disposition/status records
5. `docs/PHASE_3_STATUS.md`
6. objection-transfer blocker status
7. #13, #16, #76

The amendment should:

- freeze exact field sizes, byte order, magics/domains, kinds, snapshot/footer relationships, and selected catalog grammar;
- derive every capacity/minimum from the selected tables;
- specify canonical bulk/rewrite grouping and persistent mutation algorithms;
- specify deletion borrower/merge rules;
- specify strict validity, linked history, recovery, and resource-policy classification;
- preserve explicit experimental compatibility/migration non-promises;
- mark first-Draft and research identities historical/non-authoritative.

# P0 — Candidate interoperability corpus

Generate a **new** corpus from the selected normative bytes. Do not promote research/Candidate 1 vectors by renaming them.

Each valid vector should include deterministic recipe/generator input, exact file length, expected identities, object/tree/catalog facts, and assurance outcomes.

## Required framing/identity coverage

At minimum:

- minimal genesis;
- one append and linked history;
- exact-end strict-valid state;
- interrupted/torn/trailing publication cases;
- bounded recovery candidate(s);
- exact object/page/snapshot/commit hash-domain vectors;
- one-byte corruption of every structural magic;
- non-zero reserved/unknown flag failures.

## Required object/locator coverage

At minimum:

- minimum object;
- policy-bounded large test object;
- object digest mismatch;
- locator offset/length contradiction;
- object-header ObjectId mismatch;
- kind zero invalid;
- accepted opaque low/high application kinds.

## Required occupancy/tree-shape coverage

For selected `C` and `M`, pin boundaries around:

```text
1
C - 1
C
C + 1
2C - 1
2C
2C + 1
final group M - 1 / M / M + 1
C * internal_fanout - 1 / exact / +1
final internal group M - 1 / M / M + 1
```

Also include canonical final-two redistribution, leaf/internal split, root growth, under-minimum invalid cases, recursive repair, and root collapse.

## Required mutation coverage

At minimum:

- insertion without split;
- leaf/internal split propagation;
- deletion without repair;
- each one-donor borrow case;
- D4 two-donor policy-significant deletion;
- merge and recursive internal repair;
- canonical mixed batch;
- equivalent caller operation orders producing identical canonicalized transition bytes;
- page reuse accounting where unchanged pages should survive.

## Required catalog coverage if D5 is accepted

At minimum:

- catalog-only zero-root genesis;
- one/multiple roots;
- stable catalog ID across unchanged/replaced linked snapshots;
- changed linked-child catalog ID invalid;
- delete to catalog-only state;
- attempted catalog-slot deletion invalid;
- known/unknown optional/unknown required capability outcomes;
- unknown extension preservation;
- missing root with outer integrity otherwise valid;
- malformed root/capability/extension ordering/length/padding.

## Required determinism coverage if D7 is selected

At minimum:

- fresh construction from different caller input orders -> identical normalized bytes;
- canonical rewrite normalization;
- persistent-history pair with equal logical active set but permitted root/byte divergence;
- rewrite produces new byte/commit identity even when semantic active facts match.

# Draft → Review gate

FCP-0003 may move Draft→Review only when:

- [ ] D1–D7 have explicit maintainer dispositions committed.
- [ ] The coordinated normative amendment is merged and internally consistent.
- [ ] Every length/capacity derives from selected byte tables.
- [ ] Candidate 1/FCP-0002 disposition is consistent across status/proposal artifacts.
- [ ] The candidate valid/invalid corpus is generated from selected bytes.
- [ ] Rust/in-repository checks reproduce the corpus and safety gates remain green.
- [ ] Rejected alternatives and compatibility non-promises remain explicit.
- [ ] Phase 4 has introduced no dependency on unresolved successor bytes.

# Review → experimental-allocation gate

Review status is **not** `UCOF-EXP-0003` allocation.

Before allocation require:

- Review corrections resolved;
- one meaningful clean-room interpretation/reproduction of the normative byte tables/candidate corpus independent of reference-source implementation details;
- mismatch classification as spec/reference/independent/vector defects;
- explicit maintainer allocation decision.

# P1 — Rebase the reference implementation

Only after normative bytes are selected:

- implement the accepted byte grammar exactly;
- isolate old Candidate 1/current-research identities as historical evidence;
- regenerate candidate/authoritative vectors from specification rules;
- keep parsing/writing separate from transport/filesystem/transforms/schemas/crypto/profiles;
- preserve Rust 1.85, i686, powerpc64, docs, Clippy, fuzz, property, and adversarial gates.

The reference implementation must implement the specification rather than silently define it.

# P1 — Independent implementation / clean-room evidence

Primary tracker: #12.

Before experimental allocation, require at least one meaningful clean-room interpretation/reproduction event against the candidate corpus.

For Phase 3 exit, retain the stronger #12 gate: a separately maintained implementation or documented external clean-room review with material disagreements recorded and classified rather than silently changed to match Rust.

# P1 — Maintained remote-source qualification

Primary tracker: #10.

Required work includes:

- maintained HTTP range adapter with strong version semantics;
- one versioned cloud-object adapter;
- native asynchronous cancellation;
- operation-wide request/byte/retry/allocation/deadline/cancellation budgets;
- fail-closed range/body/version handling;
- explicit TLS/credential/redirect/proxy/cache/decompression policy;
- provider-specific request/byte/latency evidence.

Transport mechanics remain implementation/service requirements rather than EXP-0003 bytes unless the format itself depends on them.

# P1 — Production-candidate writer/publication subsystem

Primary tracker: #11.

Required work includes:

- descriptor-relative safe filesystem operations;
- private staging;
- encrypted spill where policy requires it;
- authenticated staged/restart state;
- bounded memory/bytes/files/descriptors/merge/cleanup work;
- deterministic final UCOF bytes independent of spill ciphertext randomness;
- no-overwrite publication;
- explicit not-published / published-durable / publication-indeterminate outcomes;
- platform/filesystem-specific durability qualification.

# P1 — Semantic compaction/profile boundary

Converge the existing semantic/profile work into one explicit contract:

- Core determines structural reachability;
- profile/application resolver supplies semantic dependency edges;
- unknown semantic dependencies fail closed or conservatively retain under explicit policy;
- retention/history policy is caller/profile controlled;
- rewrite/compaction produces new identity;
- byte-scoped signatures/provenance are not falsely reported as preserved through changed bytes.

# Completion rule

The **EXP-0003 Interoperability Candidate** milestone is coherent only after policy dispositions, normative bytes, candidate corpus, reference implementation, and required independent evidence agree.

Phase 3 itself remains broader and does not exit until #10–#12 and the other claimed implementation/profile/publication qualifications are satisfied.

EXP-0003 remains disposable and must not imply UCOF 1.0 compatibility.
