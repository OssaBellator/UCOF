# EXP-0003 Coordinated Normative Amendment Map

**Status:** review-only preparation; D1–D7 remain pending  
**Date:** 2026-08-13  
**Related:** `FCP_0003_DRAFT_TO_REVIEW_LEDGER.md`, `EXP_0003_CANDIDATE_CORPUS_SCAFFOLD.md`, issues #13, #16, #76

## Purpose

The Draft→Review ledger deliberately forbids piecemeal normative edits while the maintainer ballot is unresolved.

That avoids one class of drift, but it creates a second risk: after the ballot, a large coordinated amendment could accidentally update one field table while leaving a dependent snapshot length, occupancy example, catalog width, deletion rule, or status document stale.

This map prepares the **dependency graph and affected-section inventory only**.

It does not select D1–D7, edit FCP-0003, edit `spec/experimental/UCOF-EXP-0003.md`, generate wire bytes, or create authoritative vectors.

## Reproduction

```console
python3 tools/plan_exp0003_normative_amendment.py --verify
```

The tool emits:

- all D1–D7 states as `pending`;
- the exact document/section families affected by the coordinated amendment;
- both explicit D2 geometry alternatives;
- the arithmetic consequences of D2 × D5 without choosing either decision;
- cross-decision constraints;
- the required post-ballot sequencing.

## Decision dependency graph

### D1 — Candidate 1 / FCP-0002 disposition

Direct byte effect: none.

Affected governance/status artifacts include:

- Candidate 1 disposition record;
- FCP-0002 status/disposition notice;
- Phase 3 status;
- issues #13 and #76.

D1 must not be inferred from technical CI success.

### D2 — ObjectId and primary-directory geometry

D2 directly fixes:

- ObjectId width;
- object-header length;
- page-header length;
- leaf locator width;
- internal child-reference width.

It also **derives**:

- D3 page capacities/minimum occupancies/split counts;
- D5 catalog ObjectId/root-ID width;
- D5 snapshot length when catalog v2 is selected.

The explicit Review alternatives remain:

```text
tight64 full-range:
  ObjectId                8
  object/page header     40 / 40
  leaf/internal entry    56 / 56
  leaf/internal C/M     291 / 146

tight128 full-range:
  ObjectId               16
  object/page header     48 / 56
  leaf/internal entry    64 / 72
  leaf C/M              255 / 128
  internal C/M          226 / 113
```

No alternative is accepted by this map.

### D3 — occupancy / grouping / split

D3 fixes tree-shape algorithms, but its numeric constants are derived from D2.

The coordinated amendment must never copy old first-Draft numeric examples forward independently of the selected geometry.

### D4 — deletion borrower policy

D4 does not change fixed field widths.

It changes byte-significant persistent output in cases where both siblings can legally lend. The selected algorithm must agree across:

- FCP prose;
- EXP-0003 specification algorithm;
- occupancy/deletion companion wording;
- candidate corpus expected transition bytes.

### D5 — catalog / roots / capabilities / extensions

If catalog v2 is selected, D5 adds one `ObjectId` to the snapshot and defines the catalog payload grammar.

The selected ObjectId width comes from D2.

If D5 is omitted from EXP-0003, the current no-catalog structural snapshot remains 96 bytes unless Review selects another explicit design.

If D5 is selected:

```text
tight64  -> snapshot 104 bytes
tight128 -> snapshot 112 bytes
```

The footer's required `snapshot_length` must move with the selected snapshot length.

D5 also requires one Core-recognized catalog object kind. If the recommended D6 package is revised, the replacement kind policy must still satisfy that dependency when D5 is selected.

### D6 — hash domains / magics / kinds

D6 fixes:

- digest algorithm policy;
- exact domain-prefix bytes;
- structural magic values;
- page-kind validity;
- object-kind validity/opacity rules.

Under the recommended package it does not change the 32-byte digest field width.

### D7 — scoped determinism

Under the recommended scoped-determinism option, D7 does not change fixed field widths.

It changes the normative identity claim and the required corpus cases:

- fresh/rewrite normalization is current-state canonical;
- persistent transition is deterministic from exact prior state + canonicalized batch;
- equal logical active state may have different persistent root/snapshot identity across histories.

A replacement history-independent option would require a complete placement/reference/randomness proposal before this map could derive its byte consequences.

## D2 × D5 arithmetic matrix

This table is illustrative arithmetic only.

| D2 geometry | D5 catalog v2 | ObjectId | Snapshot | Footer required snapshot length |
|---|---|---:|---:|---:|
| tight64 | omitted | 8 | 96 | 96 |
| tight64 | selected | 8 | 104 | 104 |
| tight128 | omitted | 16 | 96 | 96 |
| tight128 | selected | 16 | 112 | 112 |

The planner verifies these values without selecting a row.

## Normative document map

The coordinated amendment must review at least the following artifacts together.

### `docs/proposals/0003-immutable-page-successor.md`

Review/update:

- wire-policy summary;
- identifiers;
- object header;
- primary locator;
- page geometry;
- occupancy;
- insertion;
- deletion;
- catalog/capability policy;
- scoped determinism;
- Draft→Review gates.

### `spec/experimental/UCOF-EXP-0003.md`

Review/update at least:

- §4.2 Object identifiers;
- §4.7 active-tree / catalog-only semantics;
- §5 cryptographic identity;
- §6 constants;
- §8 object record;
- §9 directory page envelope;
- §10 leaf locator;
- §11 internal child reference;
- §12 snapshot;
- §13 footer `snapshot_length`;
- §15 occupancy;
- §16 insertion;
- §17 deletion;
- §19 bulk/rewrite versus persistent identity;
- §21 strict validation;
- §22 targeted lookup/absence assurance wording;
- history/recovery/rewrite catalog checks if D5 is selected;
- catalog/capability/extension grammar if D5 is selected.

### `docs/spec/IMMUTABLE_SUCCESSOR_OCCUPANCY_POLICY.md`

Derive/update:

- selected capacities/minima;
- final-two-group examples;
- root exceptions;
- overflow splits;
- deletion repair wording.

### Governance/status artifacts

Review/update together:

- `docs/PHASE_3_DISPOSITION_DRAFT.md` → actual D1 decision record or successor record;
- `docs/PHASE_3_STATUS.md`;
- `docs/EXP_0003_INTEROP_PLAN.md`;
- `docs/review/FCP_0002_TO_0003_OBJECTION_TRANSFER.md`;
- `docs/review/FCP_0003_DRAFT_TO_REVIEW_LEDGER.md`;
- issues #13, #16, #76.

## Post-ballot execution order

After D1–D7 are explicitly dispositioned:

1. record the selected decisions in the ledger/decision record;
2. apply one coordinated normative amendment across the mapped artifacts;
3. run consistency checks over every selected length/capacity/algorithm;
4. promote applicable #119 recipe IDs into concrete deterministic candidate byte generators;
5. generate the candidate valid/invalid corpus;
6. reproduce it in the in-repository implementations/spec checks;
7. move FCP-0003 Draft→Review only when the selected normative bytes and candidate corpus agree;
8. require a meaningful clean-room interpretation/reproduction before explicit experimental allocation.

Reference-implementation migration remains after byte selection rather than driving the selection.

## CI guardrail

`--verify` asserts:

- D1–D7 remain `pending`;
- both D2 alternatives derive their known capacities/minima/splits;
- all four D2×D5 snapshot combinations derive correctly;
- no emitted state claims `accepted=true`, `authoritative=true`, or `allocated=true`;
- Draft→Review remains after normative/spec/corpus agreement.

## Boundary

This map is not a normative amendment.

It does not:

- select or record a maintainer disposition;
- alter current Draft bytes;
- accept FCP-0003;
- supersede Candidate 1;
- allocate EXP-0003;
- generate candidate or authoritative wire bytes;
- migrate the Rust implementation;
- begin Phase 4 wire work.
