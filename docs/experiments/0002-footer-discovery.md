# Experiment 0002 — Exact-end versus bounded-backward footer discovery

**Status:** Reproducible Phase 1 evidence  
**Script:** `tools/experiment_footer_discovery.py`  
**Scope:** `UCOF-EXP-0001` only

## Question

Should a reader require the footer at the exact end of the file, or search backward within a bounded tail to tolerate trailing bytes and interrupted publication?

## Compared strategies

**Exact-end discovery** reads the final 80 bytes and accepts one possible footer position.

**Bounded-backward discovery** scans up to 64 KiB plus one footer length and treats each matching magic value as a candidate requiring structural and integrity validation.

The experiment counts bytes examined and candidate positions. It does not claim wall-clock performance.

## Results

| Case | File bytes | Exact candidates | Backward candidates | Exact bytes examined | Backward bytes examined |
|---|---:|---:|---:|---:|---:|
| exact | 180 | 1 | 1 | 80 | 180 |
| trailing-16 | 196 | 0 | 1 | 80 | 196 |
| trailing-64KiB | 65,716 | 0 | 1 | 80 | 65,616 |
| dense-fake-magics | 65,716 | 1 | 8,184 | 80 | 65,616 |

## Interpretation

Backward search can recover a valid older footer hidden by trailing bytes. It also creates candidate ambiguity and attacker-controlled validation amplification. A dense 64 KiB tail can present thousands of apparent magic positions before deeper validation rejects them.

Exact-end discovery has a simpler contract:

- one location;
- constant tail read;
- no candidate ordering rule;
- no accidental acceptance of an older root hidden before trailing data.

Its limitation is equally clear: it cannot recover from trailing garbage or an interrupted append without a separate checkpoint or recovery mechanism.

## Decision for EXP-0001

Keep exact-end discovery. Recovery must not be added by silently searching backward in normal validation.

Phase 3 should distinguish three operations:

1. strict active-root validation at an unambiguous location;
2. bounded checkpoint discovery with explicit ordering and validation rules;
3. salvage mode that reports candidates without promoting them to active valid state.

Each operation needs separate resource limits and diagnostics.
