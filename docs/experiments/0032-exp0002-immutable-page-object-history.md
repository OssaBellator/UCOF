# Experiment 0032: Complete-Object Insert, Delete, and Verified History

- **Status:** Reproducible byte-level prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0023, 0027, and 0031
- **Script:** `tools/experiment_exp0002_immutable_page_object_history.py`

## Question

Can complete objects be inserted and deleted through immutable directory pages with deterministic append-only publication, and what assurance difference remains between validating only the active snapshot and validating every linked historical prefix?

## Sequence

The fixture starts with 10,000 complete objects using even identifiers. It then:

1. inserts one odd-identifier object into a full leaf, exercising leaf split and immutable path copying;
2. publishes a linked exact-end snapshot;
3. deletes that object, exercising recursive immutable deletion and rebalance;
4. publishes a second linked snapshot;
5. validates the linked prefixes in reverse order as sequences 2, 1, and 0.

Both insertion and deletion are replayed from the same input and must produce byte-identical output.

## Assurance boundary

The deleted object's bytes remain physically present in the append-only history but are not reachable from the latest active directory.

The experiment mutates that deleted object's payload after sequence 2 is published:

- active-snapshot validation still succeeds, because the latest snapshot no longer references the object and the mutation lies outside the latest commit span;
- verified-history validation fails when it validates the exact sequence-1 prefix and reaches the object's authenticated digest.

This is not a weakness in active validation. It is a deliberate distinction between two assurance claims:

- **active validation:** the current published object set is structurally and cryptographically consistent;
- **verified history:** every linked historical state and every object reachable from those states is also consistent.

Tools must not label active-only validation as verified history.

## Interruption behavior

Removing half of the final deletion footer causes the latest file to fail exact-end validation. The complete sequence-1 prefix remains independently valid.

## Findings

1. Complete-object insertion and deletion can use immutable path copying without rewriting unrelated objects or pages.
2. Append-only deletion removes reachability, not historical bytes.
3. Active and history validation require separate API names and result types.
4. A deleted historical object can be outside the latest commit digest and latest active directory while remaining security-relevant to history claims.
5. Verified history must validate each exact ancestor prefix rather than trusting only parent metadata carried by the newest footer.

## Boundaries

This prototype validates history by slicing each exact ancestor prefix in memory. Bounded random-access history traversal, cancellation, source-version stability, cross-language vectors, recovery, history-retention policy, and authenticity remain future work.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_object_history.py
```
