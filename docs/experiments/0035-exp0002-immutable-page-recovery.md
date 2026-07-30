# Experiment 0035: Bounded Recovery and Verified Source History

- **Status:** Reproducible bounded source prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0018, 0032, and 0033
- **Script:** `tools/experiment_exp0002_immutable_page_recovery.py`

## Question

Can an interrupted immutable-page append expose earlier complete prefixes through bounded recovery without treating footer magic as authority, hiding failed-candidate cost, or silently selecting one recovered state?

## Fixture

The fixture publishes three complete states:

1. genesis sequence 0;
2. insertion sequence 1;
3. deletion sequence 2.

It then appends a replacement object whose payload contains eight fake footer-magic strings and truncates sequence 3 halfway through its footer.

Exact-end validation of the resulting file must fail. Recovery is a separate operation.

## Recovery algorithm

Recovery:

- reads only a caller-bounded suffix;
- finds every footer-magic occurrence within a bounded match count;
- treats truncated matches as non-authoritative evidence;
- validates each complete candidate as an exact prefix through the bounded source validator from Experiment 0033;
- charges scan reads, failed candidates, successful candidates, page traversal, commit hashing, and object hashing to one cumulative source budget;
- reports every valid prefix found within the configured bounds;
- performs no automatic ranking or selection.

The expected recovered sequences are 2, 1, and 0 in newest-to-oldest scan order. The eight payload matches must consume candidate-validation work and fail.

## Verified history

Starting from a caller-selected recovered prefix, verified history:

- strictly validates the exact current prefix;
- reads its exact footer and snapshot;
- cross-checks sequence decrement and parent snapshot digest;
- moves to the exact previous-footer prefix;
- repeats until sequence 0;
- enforces an explicit maximum history-entry count.

Each ancestor is revalidated as a complete exact-end file. Parent metadata in the latest footer is not treated as proof that the ancestor bytes remain valid.

## Limit behavior

The experiment proves:

- a candidate-validation limit fails rather than claiming an exhaustive report;
- a suffix window containing only the truncated newest footer returns no valid prefixes;
- a history-entry limit stops traversal before the next ancestor;
- cumulative reads never reset between failed candidates.

## Findings

1. Footer magic is candidate evidence, not publication authority.
2. Exact-end validation must never fall back to recovery.
3. Recovery reports candidates and leaves selection to an explicit caller policy.
4. Failed candidates are attacker-controlled work and must consume the same global budget as successful candidates.
5. Verified source history must revalidate every exact ancestor prefix.

## Boundaries

This prototype uses an in-memory version-stable source. It does not yet combine recovery with a live conditional HTTP adapter, asynchronous cancellation, diagnostic ranking, authenticity, or external freshness. Recovery output is evidence, not a repaired file.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_recovery.py
```
