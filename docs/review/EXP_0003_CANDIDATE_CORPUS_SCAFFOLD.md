# EXP-0003 Candidate Corpus Scaffold

**Status:** review-only recipe plan; no authoritative bytes or hashes  
**Date:** 2026-08-13  
**Related:** `FCP_0003_DRAFT_TO_REVIEW_LEDGER.md`, issues #13, #16, #76

## Purpose

The Draft→Review ledger requires a new candidate valid/invalid corpus **after** D1–D7 receive explicit maintainer dispositions and one coordinated normative amendment freezes exact bytes.

At the same time, waiting until after the ballot to discover missing boundary cases would create avoidable churn.

This scaffold prepares the **case inventory and derived numeric boundaries only** while the decisions remain pending.

It deliberately does not generate:

- EXP-0003 files;
- object/page/snapshot/commit digests;
- expected whole-file hashes;
- authoritative vector manifests;
- accepted-policy labels.

## Tool

```console
python3 tools/plan_exp0003_candidate_corpus.py --verify
```

The tool emits JSON plans for both explicit D2 geometry alternatives by default.

Optional views:

```console
python3 tools/plan_exp0003_candidate_corpus.py --geometry tight64 --verify
python3 tools/plan_exp0003_candidate_corpus.py --geometry tight128 --verify
```

Every emitted plan carries:

```text
status = review-only candidate corpus scaffold; no authoritative bytes or hashes
D1..D7 selection_state = pending
```

and every case is labeled `recipe-only`.

## Geometry alternatives represented

### Tight 64-bit full-range Review alternative

Derived directly from the D2 packet:

```text
ObjectId width              8
object header              40
page header                40
leaf entry                 56
internal entry             56
page size              16,384
leaf C/M                  291 / 146
internal C/M              291 / 146
leaf overflow             292 -> 146,146
internal overflow         292 -> 146,146
catalog-v2 snapshot       104 if D5 is selected
```

### Tight 128-bit full-range Review alternative

```text
ObjectId width             16
object header              48
page header                56
leaf entry                 64
internal entry             72
page size              16,384
leaf C/M                  255 / 128
internal C/M              226 / 113
leaf overflow             256 -> 128,128
internal overflow         227 -> 114,113
catalog-v2 snapshot       112 if D5 is selected
```

Both remain review alternatives. The scaffold does not encode the tight-64 recommendation as acceptance.

## Derived occupancy recipes

For each geometry, the tool derives object-count boundary recipes around:

```text
1
C - 1
C
C + 1
2C - 1
2C
2C + 1
C + M - 1
C + M
C + M + 1
C * internal_fanout - 1
C * internal_fanout
C * internal_fanout + 1
```

It also emits explicit leaf-group boundaries:

```text
M - 1
M
M + 1
C
C + 1
```

and internal child-count boundaries around both minimum and fanout transitions.

These are recipe inputs, not expected tree identities. The eventual accepted generator/validator must derive the canonical grouping and exact bytes from the normative algorithm.

## Decision-neutral policy branches

Some corpus cases only make sense after a D1–D7 choice. The scaffold keeps those conditions visible instead of silently applying recommendations.

### D4 deletion policy

Both mutually exclusive two-donor recipes remain present:

```text
mutation.two-donor-left-first
mutation.two-donor-fuller-left-tie
```

The accepted candidate corpus will retain only the case(s) appropriate to the selected policy, plus negative/differential evidence where useful.

### D5 catalog v2

Catalog recipes remain explicitly conditional on D5, including:

- zero-root genesis;
- one/multiple roots;
- stable catalog ID through unchanged/replaced child snapshots;
- changed linked-child catalog ID invalid;
- delete to catalog-only state;
- catalog-slot deletion invalid;
- capability support outcomes;
- unknown-extension preservation;
- missing-root semantic invalidity;
- malformed ordering/length/padding.

If D5 is rejected or removed from the epoch, those recipes do not become accidental Core requirements.

### D6 hash/magic/kind package

Exact domain/magic/kind recipes are marked conditional on D6 rather than treated as selected merely because the current Draft contains the same proposed constants.

### D7 scoped determinism

The scaffold always includes fresh caller-order normalization and canonical rewrite cases.

The explicit persistent equal-logical-state / byte-divergence recipe remains conditional on selecting scoped determinism.

A future history-independent persistent alternative cannot receive a made-up recipe here because no complete placement/reference/randomness proposal exists yet.

## Stable case IDs

Recipe IDs are intended to remain stable through the Review ballot where semantics are unchanged, even if numeric counts differ by selected geometry.

Examples:

```text
occupancy.objects.c-plus-1
mutation.recursive-internal-repair
catalog.zero-root-genesis
determinism.canonical-rewrite-normalization
```

The future byte generator can use these IDs as manifest keys while deriving expected hashes only after normative bytes are frozen.

## CI invariants

`--verify` asserts:

- tight-64 derives `291/146` for both page kinds and `292 -> 146/146` overflow;
- tight-128 derives leaf `255/128`, internal `226/113`, and the corresponding overflow splits;
- catalog-v2 conditional snapshot lengths derive to 104/112;
- every D1–D7 selection state remains `pending`;
- no duplicate recipe IDs exist within one geometry plan;
- every case remains `recipe-only`;
- the emitted JSON does not claim `authoritative=true` or `accepted=true`;
- both mutually exclusive D4 two-donor alternatives remain present;
- D5 and D7 conditional recipes remain visible rather than being silently selected.

## Promotion rule after maintainer dispositions

This scaffold must **not** be renamed or treated as the authoritative corpus.

After D1–D7 and the coordinated normative amendment:

1. select the accepted geometry/policies explicitly;
2. turn applicable recipe IDs into concrete deterministic byte generators;
3. generate exact candidate files and invalid mutations from the specification;
4. record expected byte lengths, structural facts, scoped digests, and outcome classes;
5. validate independently in Rust/Python/clean-room implementations;
6. only after the applicable governance gate call the selected corpus authoritative for the disposable epoch.

Historical/rejected policy recipes may remain in a differential research corpus, but they must not be confused with the selected epoch contract.

## Boundary

This scaffold does **not**:

- select D1–D7;
- edit FCP-0003 or the EXP-0003 Draft;
- generate EXP-0003 wire bytes;
- create authoritative hashes;
- change current Rust research bytes;
- move FCP-0003 to Review;
- allocate `UCOF-EXP-0003`;
- start Phase 4.

It is only mechanical preparation so the post-ballot corpus work begins from an explicit, reproducible case inventory rather than an ad hoc list.