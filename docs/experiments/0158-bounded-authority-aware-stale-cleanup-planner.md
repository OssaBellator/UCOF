# Experiment 0158 — bounded authority-aware stale cleanup planner

**Status:** non-normative Phase 3 planning evidence; **no filesystem deletion is performed**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0154–0157

## Purpose

Long-lived private writer state needs a cleanup policy that is both resource-bounded and publication-authority-aware. A stale scan must not grow without limit, and age alone must never turn an indeterminate publication into an automatic deletion.

This experiment implements a planner only. It consumes candidate facts and emits a bounded set of proposed actions. It intentionally performs no filesystem mutation.

The most important safety invariant is categorical:

> authenticated `ResolvePublication` authority never becomes a cleanup or quarantine action, regardless of candidate age or byte size.

Such state is retained for explicit publication resolution.

## Candidate facts

Each candidate carries:

- operation identity;
- age in caller-defined ticks;
- metadata bytes charged to the scan budget;
- private bytes charged to the cleanup budget;
- trust classification.

Trust is one of:

- authenticated authority;
- unauthenticated;
- malformed.

Authenticated authority is one of:

- `ResumeOrDiscardPrivate`;
- `ResolvePublication`;
- `CleanupDurablePrivate`;
- `TerminalDiscarded`.

These are planner inputs in this experiment. A production path must derive them from authenticated journal/restart state rather than accept caller assertions.

## Hard limits

Planning requires nonzero limits for:

- stale-age threshold;
- maximum scan entries;
- maximum scanned metadata bytes;
- maximum total actions;
- maximum cleanup entries;
- maximum cleanup bytes;
- maximum quarantine entries.

A zero-limit configuration is rejected before scanning.

All counters use checked accounting.

## Bounded scan

A million-candidate regression feeds stale authenticated candidates through an iterator while `max_scan_entries = 32`.

The planner consumes only 32 candidates, emits at most the configured cleanup actions, records the rest of the scanned candidates as retained by budget, and marks the scan truncated.

The planner does not collect the input stream before processing it.

A second regression sets a metadata-byte budget that fits one candidate but not the next. The planner stops before charging or acting on the oversized next candidate.

Therefore both candidate count and metadata bytes bound discovery work.

## Publication-indeterminate authority

Two deliberately extreme `ResolvePublication` candidates are supplied, including one with `age_ticks = u64::MAX` and `private_bytes = u64::MAX`.

The planner requires:

- `retained_for_resolution = 2`;
- zero cleanup entries;
- zero quarantine entries;
- an empty action list.

Neither extreme age nor extreme storage pressure weakens publication authority.

## Cleanup authority

For stale authenticated candidates with positive private bytes:

- `ResumeOrDiscardPrivate` may produce `DiscardPrivate`;
- `CleanupDurablePrivate` may produce `CleanupDurablePrivate`;
- `TerminalDiscarded` may produce `CleanupDiscardedRemnants`.

An action is emitted only when both cleanup-entry and cleanup-byte budgets permit it and the shared action cap is not exhausted.

A regression sets `max_cleanup_entries = 2` and `max_cleanup_bytes = 150`, feeds candidate sizes 100, 50, and 1, and requires only the first two actions. The third candidate is retained because the bounded cleanup budget is exhausted.

## Untrusted state

Unauthenticated or malformed candidate state is never converted directly into a destructive action.

It may produce only `QuarantineForReview`, bounded by both the quarantine cap and the shared total-action cap. Once either cap is exhausted, the candidate is retained rather than acted upon.

This keeps unverifiable state fail-closed.

## Fresh state

Candidates younger than the stale threshold are retained without action even when cleanup budget is available.

Age qualification occurs before action selection.

## Shared action cap

Cleanup and quarantine actions consume the same `max_actions` budget.

A regression mixes an authenticated cleanup candidate, an unauthenticated quarantine candidate, and a second authenticated cleanup candidate under `max_actions = 2`. Only two actions are emitted; the third candidate is retained by budget.

This prevents separate subsystem caps from creating an unexpectedly larger total mutation plan.

## What this closes

The planner establishes executable policy evidence that:

- stale discovery is bounded by entry and metadata-byte limits;
- proposed mutation work is bounded by total actions, cleanup entries, cleanup bytes, and quarantine entries;
- fresh state is not cleaned merely because capacity exists;
- unauthenticated/malformed state is never directly deleted;
- publication-indeterminate authority is categorically non-destructive;
- large age or byte values cannot override `ResolvePublication` safety;
- accounting overflow and invalid zero-limit configurations fail closed.

## What remains open

This is not yet a cleanup executor. Production work still requires:

- deriving candidate trust and authority from an authenticated journal rather than caller-supplied facts;
- binding any destructive authorization to operation identity and exact journal generation;
- binding it to expected artifact identity and expected private-byte accounting;
- revalidating journal authority and artifact identity immediately before mutation;
- refusing execution if state changed between planning and execution;
- descriptor-pinned or equivalent hardened filesystem handles;
- race-resistant removal semantics stronger than pathname re-resolution;
- cleanup-result journaling and restart semantics;
- physical power-loss/filesystem qualification.

The existing Linux descriptor-pinned staging evidence is useful for the storage side, but its documented final check/unlink name-race boundary remains relevant and must not be hidden by this planner.

## Verification

Implementation head `df2e22f00a071766707f50ebd15af5663c0a56bd` is green on the decisive Experiment 0158 gates in Rust workflow run `31782471041`:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all bounded stale-cleanup planner regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The workflow continued through the repository's broader transport, parser, vector, policy, and framing replay after the planner implementation gate passed; those broader checks do not change the cleanup-policy result.

## Next executable slice

The next slice should turn a bounded proposed cleanup action into a **generation-bound execution authorization**, not directly into a filesystem delete.

A safe authorization token should bind at least:

- operation identity;
- authenticated journal generation;
- expected cleanup authority;
- action kind;
- expected artifact identity;
- expected private-byte charge.

Execution must reload/revalidate authenticated journal state and artifact identity immediately before mutation. Any generation change, authority change, artifact replacement, authentication failure, or `ResolvePublication` state must reject the action with zero destructive effect.

Only after that authorization model is green should it be connected to the descriptor-pinned storage backend.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
