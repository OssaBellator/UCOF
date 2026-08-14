# Experiment 0161 — authenticated bounded cleanup pipeline

**Status:** non-normative Phase 3 pipeline evidence; **destructive effect remains simulated**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0158–0160

## Purpose

Experiments 0158 and 0160 separately established bounded stale-cleanup planning and generation-bound execution authorization. Keeping those proofs separate leaves an avoidable trust seam: a caller could potentially assert planner trust/authority independently from the journal state used to authorize execution.

This experiment consolidates those responsibilities into one executable path.

The planner now authenticates each candidate journal itself, derives the only legal destructive action from that authenticated authority, applies bounded scan/action/byte/quarantine limits, and emits the exact generation/artifact-bound authorization token consumed by the executor. The caller no longer supplies a trusted authority classification or destructive action.

The executor still performs only a simulated destructive effect by changing an in-memory artifact from present to absent. No filesystem deletion occurs.

## Authenticated candidate input

Each candidate carries:

- candidate identity used only for bounded quarantine reporting;
- age in caller-defined ticks;
- metadata bytes charged to scan budget;
- a sealed journal;
- current artifact identity, private-byte charge, and presence state.

The sealed journal contains:

- operation identity;
- journal generation;
- cleanup authority.

The test authenticator uses SHA-256 domain-separated tags solely as executable plumbing. It is **not a production MAC/AEAD, is not a confidentiality mechanism, and is not a cryptographic security claim**.

## Authority-derived action

After successful journal authentication the planner derives the action internally:

- `ResumeOrDiscardPrivate` -> `DiscardPrivate`;
- `CleanupDurablePrivate` -> `CleanupDurablePrivate`;
- `TerminalDiscarded` -> `CleanupDiscardedRemnants`;
- `ResolvePublication` -> no destructive action.

No action parameter is accepted from the candidate or caller.

Therefore an authenticated authority cannot be paired with a more permissive caller-selected action during planning.

## Hard planning limits

The pipeline requires nonzero limits for:

- stale-age threshold;
- maximum scan entries;
- maximum scanned metadata bytes;
- maximum total actions;
- maximum authorized cleanup entries;
- maximum authorized cleanup bytes;
- maximum quarantine entries.

The total action cap is shared by authorized cleanup tokens and quarantine actions.

All accounting uses checked arithmetic.

## Million-candidate bounded stream

A regression supplies an iterator over 1,000,000 stale candidates whose journals all authenticate as `ResumeOrDiscardPrivate`.

With `max_scan_entries = 32` and `max_authorized_entries = 8`, the planner requires:

- exactly 32 candidates scanned;
- exactly 8 authorization tokens emitted;
- 24 scanned candidates retained because the authorization budget is exhausted;
- `scan_truncated = true`;
- every emitted action to be an authenticated authorization token.

The input workload is not collected before planning.

## Authenticated action derivation

A regression supplies one candidate for each destructive authority and opens the resulting authorization tokens.

The actions must be exactly:

1. `DiscardPrivate`;
2. `CleanupDurablePrivate`;
3. `CleanupDiscardedRemnants`.

This demonstrates that action selection is a function of authenticated authority rather than caller input.

## ResolvePublication, fresh, and missing state

A regression combines:

- a `ResolvePublication` candidate with `u64::MAX` age and private-byte values;
- a fresh candidate below the stale threshold;
- a stale candidate whose artifact is absent.

The planner emits zero destructive tokens and records each case in its corresponding retained class.

Extreme age or storage pressure therefore still cannot turn `ResolvePublication` into cleanup authority.

## Unauthenticated journal handling

A regression tampers six sealed journals after authentication tags are created.

With a quarantine cap of four, the planner requires:

- zero cleanup authorizations;
- four bounded `QuarantineForReview` actions;
- two additional candidates retained because quarantine/action budget is exhausted.

Unauthenticated state never becomes a destructive token.

## Authorization entry and byte bounds

A regression sets:

- `max_authorized_entries = 2`;
- `max_authorized_bytes = 150`.

Candidates with private-byte charges 100, 50, and 1 are processed. Only the first two receive authorization, the cumulative authorized charge is exactly 150, and the third candidate is retained.

Entry and byte limits are both hard.

## Shared action cap

A regression mixes:

- one valid authenticated cleanup candidate;
- one tampered journal that becomes quarantine-only;
- one additional valid cleanup candidate.

Under `max_actions = 2`, the plan contains exactly one authorization and one quarantine action. The third candidate is retained.

Separate subsystems cannot multiply the caller's requested action bound.

## Planner-to-executor handoff

A token emitted directly by the bounded planner is passed to the Experiment 0160-style executor.

With unchanged authenticated journal state and unchanged artifact identity/byte count, the derived action executes once and the simulated artifact becomes absent.

No intermediate caller reconstruction of authority/action claims occurs.

## Post-plan TOCTOU revalidation

Two regressions mutate state after planning:

- advancing authenticated journal generation;
- replacing artifact identity.

Both fail at execution and leave the artifact present.

A third regression changes current authenticated authority to `ResolvePublication` after planning. Execution returns `ResolvePublication` and leaves the artifact present.

Thus bounded planning does not weaken last-moment execution revalidation.

## Metadata scan bound and invalid configuration

A metadata-byte budget that fits one 64-byte candidate but not a second stops before scanning the second candidate and marks the scan truncated.

A zero total-action limit is rejected before planning begins.

## What this closes

This experiment removes the caller-controlled trust seam between stale cleanup planning and execution authorization:

- candidate journals are authenticated by the planner itself;
- destructive action is derived only from authenticated authority;
- `ResolvePublication` is categorically non-destructive;
- unauthenticated journals can only become bounded quarantine actions;
- scan, action, authorization-entry, authorization-byte, and quarantine limits are jointly enforced;
- the planner emits the exact generation/artifact-bound token consumed by execution;
- post-plan journal/artifact changes still fail closed;
- a million-candidate source remains bounded by configured scan/action limits.

## What remains open

This is still research pipeline evidence, not a production cleanup subsystem. Remaining gates include:

- a vetted production MAC/AEAD or equivalent authentication primitive;
- encrypted private-stage records and real key/nonce provenance;
- durable authenticated journal storage and anti-rollback authority;
- descriptor-pinned or equivalent hardened artifact handles;
- binding tokens to observed filesystem identity from the already-open/pinned artifact;
- cleanup-result journaling and restart authority;
- same-UID final-step race handling or an explicitly selected stronger isolation assumption;
- physical power-loss and filesystem qualification.

The Linux descriptor-pinned backend in PR #130 remains a useful execution target, but its current staged-name identity check and subsequent `remove_file` are separate operations. This experiment does not change or conceal that boundary.

## Verification

Implementation head `a8c26ef3bf08f4810c46492bff46b52e14430ada` is green on the decisive Experiment 0161 gates in Rust workflow run `31783708529`:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all authenticated bounded cleanup pipeline regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

After the implementation gate passed, the same workflow also completed the concrete HTTP transport, async targeted lookup/full validation/history/recovery, versioned S3 adapter, and deletion-policy trace checks successfully before continuing through the broader repository evidence replay.

## Next executable slice

The highest-value next slice is no longer another logical cleanup model. The repository now needs a **real vetted AEAD implementation** for private-stage records so Experiments 0155–0157 stop depending on a deliberately non-confidential test adapter.

The dependency must be added without weakening the repository's `--locked` gate. Because `aws-lc-rs 1.18.0` already exists in the current lock graph transitively, a direct dependency can be evaluated with a single atomic Cargo manifest/lockfile commit rather than exposing an intermediate unlocked branch state.

The AEAD experiment should preserve the existing operation-wide nonce lease and exact AAD binding, then prove real ciphertext confidentiality/integrity behavior while keeping key management, durable nonce/journal commit, and production lifecycle qualification as separate explicit gates.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
