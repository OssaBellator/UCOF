# Experiment 0021: Profile-Supplied Dependency and History Retention

- **Status:** Reproducible model
- **Date:** 2026-07-30
- **Related:** FCP-0002 repair and semantic compaction
- **Script:** `tools/experiment_exp0002_profile_retention.py`

## Question

What information must a profile or application supply before a generic UCOF tool can claim semantic compaction rather than caller-directed object selection?

## Required inputs

The model requires two independent inputs:

1. **Snapshot retention policy**
   - retain the active snapshot;
   - retain the last N verified snapshots by sequence;
   - retain explicitly pinned snapshot identities.

2. **Profile dependency resolver**
   - for every interpreted object, return its logical object dependencies;
   - identify objects whose dependency semantics are unknown.

Wall-clock age is deliberately absent. Candidate 1 has no trusted timestamp or external freshness source.

## Deterministic planning

The planner:

- validates strictly increasing snapshot sequences;
- rejects absent pinned identities;
- unions roots from selected snapshots;
- traverses dependencies iteratively with cycle-safe visited tracking;
- sorts retained and discarded identifiers;
- independently limits snapshots, nodes, edges, and dependency depth.

The output is a deterministic plan. It is not a byte writer and does not mutate the source.

## Unknown semantics

Two explicit policies are tested:

### Abort

If a retained root or dependency has unknown semantics, semantic compaction stops. This is the safest generic behavior.

### Retain all unknown objects

A conservative profile may declare that every object with unknown dependency semantics must be retained. This avoids silent loss but can reduce compaction benefit.

Silently treating an unknown object's dependency set as empty is rejected.

## History examples

The executable fixture proves different policies produce different correct retained sets:

- active-only retention;
- last-two-snapshot retention;
- active plus a pinned genesis snapshot;
- cyclic dependencies retained once;
- unknown-semantics abort;
- conservative unknown retention.

Missing dependencies and work-limit exhaustion fail closed.

## Findings

1. The generic container cannot discover arbitrary semantic dependencies from physical reachability alone.
2. Snapshot retention and object dependency traversal are separate inputs.
3. Semantic compaction needs a profile or application contract covering every retained object kind.
4. Unknown dependency semantics must abort or invoke an explicit conservative retention rule.
5. Repair-all remains distinct: it copies every object in a strictly verified active snapshot and makes no semantic-pruning claim.
6. Caller-selected rewrite remains distinct: it follows explicit identifiers but cannot prove semantic completeness.
7. History retention is based on verified sequence and identity, not untrusted wall-clock age.

## Required successor contract

A future profile-level compaction contract should define:

- dependency extraction by object kind and schema version;
- treatment of optional, weak, external, and historical references;
- snapshot retention by sequence and explicit identity;
- unknown-object fallback;
- limits and diagnostics;
- whether unknown optional extensions must be preserved;
- output root and identity rules;
- signature and provenance invalidation.

## Reproduction

```console
python3 tools/experiment_exp0002_profile_retention.py
```
