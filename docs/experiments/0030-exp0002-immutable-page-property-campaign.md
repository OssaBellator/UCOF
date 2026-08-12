# Experiment 0030: Immutable-Page Multi-Seed Property Campaign

- **Status:** Reproducible differential campaign
- **Date:** 2026-07-30
- **Related:** Experiment 0024
- **Script:** `tools/experiment_exp0002_immutable_page_property_campaign.py`

## Question

Do the height-one immutable-page insertion, deletion, split, merge, routing, and root-transition rules remain deterministic and equivalent to a sorted-set oracle across many operation sequences?

## Campaign

The campaign runs:

- 34 deterministic seeds;
- 256 operations per seed;
- 8,704 total insertions and deletions;
- the scripted split-and-collapse prefix from Experiment 0024;
- a fixed identifier space and object-count envelope.

Every seed is executed twice. The complete final byte stream, root reference, sorted identifiers, and work counters must be identical between replays.

## Invariants

After every operation, the underlying sequence runner checks:

- exact sorted-set agreement;
- unique strictly ordered identifiers;
- exact root minimum and maximum;
- valid page digests and child ranges;
- root level at or below the constrained height-one boundary;
- no more than three new pages emitted by one operation;
- deterministic root-height increase and collapse.

The campaign aggregates:

- insertions and deletions;
- new-page and reused-page observations;
- maximum per-operation page emission;
- root-height transitions;
- a SHA-256 over every deterministic final report digest.

## Why aggregate evidence

Pinning every intermediate append-only byte stream would be large and redundant. A deterministic aggregate digest provides a compact regression signal while the executable oracle still validates every intermediate operation.

The aggregate is implementation evidence, not a wire identity or normative test vector. Any algorithm or split-policy change must intentionally update the campaign result and its rationale.

## Findings

1. One fixed operation sequence is insufficient for a persistent-tree state machine.
2. Multi-seed deterministic replay finds routing, occupancy, and transition defects while remaining reproducible.
3. A sorted-set oracle validates logical contents independently of page layout.
4. Per-operation page-emission bounds can be checked alongside semantic correctness.
5. A successor still requires cargo-fuzz or equivalent arbitrary operation-sequence coverage after its implementation language and public byte contract are selected.

## Limitations

The campaign inherits Experiment 0024's height-one constraint. Recursive internal split and delete boundaries are covered separately by Experiments 0025 and 0027, not randomly combined here.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_property_campaign.py
```
