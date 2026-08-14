# Experiment 0159 — journal-backed nonce lease activation

**Status:** non-normative Phase 3 integration evidence; **durable journal commit remains an external boundary**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0156–0157

## Purpose

Experiment 0156 defines authenticated restart/discard journal authority and Experiment 0157 defines crash-safe nonce leases. This experiment joins those two contracts at the point that matters for nonce safety: a planned lease must become a new authenticated journal generation, and no nonce in that lease may become issuable until the exact candidate generation is reported durably committed.

The experiment is deliberately narrower than a production journal backend. It proves the logical handoff and crash cuts, not filesystem durability, rollback-resistant storage, or production cryptography.

## Durable journal state

The model carries:

- operation identity;
- key identity;
- journal generation;
- `next_unreserved: Option<u64>`.

A pending lease records the exact durable base state, the exact candidate next journal state, and the first/last counter covered by the lease.

The candidate journal advances generation by one and moves `next_unreserved` past the entire lease. Counter and generation arithmetic are checked; wrap is rejected.

## Activation sequence

Lease activation requires all of the following:

1. the current durable journal still equals the pending lease's recorded base state;
2. the backend reports the candidate generation durably committed;
3. the sealed candidate journal authenticates successfully;
4. the authenticated journal equals the pending lease's exact candidate state.

Only after those checks does the model create an active lease capable of issuing counters.

This enforces the executable sequence:

`pending lease -> authenticated candidate journal generation -> durable commit -> active lease`

A merely planned, serialized, or authenticated candidate is not sufficient.

## Crash before durable commit

A regression reserves counters `[0, 3]`, seals the candidate journal, but reports the candidate as not durably committed.

Activation fails as `NotDurablyCommitted`, and the prior durable state remains at `next_unreserved = 0`.

Because the pending lease has no allocation path, those counters have never become issuable and may safely be reserved again after restart from the prior durable state.

## Crash after durable commit

A regression evaluates every pre-crash use cut for a four-counter lease, from zero used counters through all four used counters.

Once candidate generation 1 is durably committed:

- its durable state has `next_unreserved = 4` regardless of how many counters were actually used;
- restart reserves the next lease beginning at counter 4;
- no counter below 4 is issued after restart;
- unused counters in the old committed lease are intentionally burned.

The result joins the nonce-lease high-water rule directly to journal generation authority.

## Exact candidate-generation binding

The authenticated journal is not accepted merely because it is newer or well-formed.

A regression seals a foreign candidate whose generation is one higher than the pending lease's exact candidate generation. Authentication succeeds, but activation fails as `CandidateMismatch`.

Therefore a caller cannot substitute a different authenticated generation for the lease it is attempting to activate.

## Authentication failure

The integration test authenticator is intentionally test-only plumbing built from a SHA-256 domain-separated tag. It is **not a production MAC or AEAD and provides no confidentiality claim**.

Within that deliberately limited boundary, regressions prove that:

- a modified sealed journal fails authentication;
- the same candidate sealed under a different test key fails authentication;
- an unauthenticated/tampered candidate cannot activate a lease.

Production integration still requires a vetted cryptographic primitive and key-management policy.

## Stale reservation race

Two pending reservations may be derived from the same durable base in the model.

If one candidate wins and advances durable generation first, activation of the other pending reservation fails because its recorded base no longer equals current durable state.

This models the compare-and-swap/single-writer property a real journal backend must provide.

## Active generation provenance

An active lease records the exact authenticated generation that authorized it.

A regression requires the active lease's `committed_generation` to equal the returned committed journal generation. This provides an explicit hook for later audit, restart, and execution-authorization checks.

## What this closes

The integration contract demonstrates that:

- nonce issuance cannot begin from a merely pending journal generation;
- the exact lease candidate must be authenticated and durably committed before activation;
- committed leases burn their entire reserved range across restart;
- authenticated but foreign generations cannot authorize a different pending lease;
- tampered or wrongly authenticated journals cannot authorize issuance;
- stale pending leases cannot activate after durable state advances;
- active nonce state retains its authorizing journal generation.

## What remains open

This experiment does **not** establish a production restart journal. Still required are:

- a vetted MAC/AEAD or equivalent authenticated-journal primitive;
- a durable, crash-consistent journal replacement and parent-directory synchronization protocol;
- a trusted rollback/generation anchor or equivalent anti-rollback policy;
- single-writer ownership or compare-and-swap semantics across processes;
- integration with real private-stage AEAD nonce construction so no code path can bypass the lease allocator;
- hardened artifact storage and stale-state execution;
- operation/key/nonce-prefix provenance across restart;
- physical power-loss and filesystem qualification.

## Verification

Implementation head `a37b87dc31324d6789765739e734ddebdc1bd30f` is green in Rust workflow run `31781382608` on:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including every journal/lease crash-cut and substitution case;
- concrete HTTP/source tests;
- parser, adversarial, policy, invalid-corpus, vector, and EXP-0003 scaffold/amendment verification;
- framing experiment replay;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

## Next executable slice

The higher-value follow-up is execution authorization for stale private state. Cleanup planning must be derived from authenticated journal authority rather than caller-supplied trust, and any destructive action token must bind operation identity, journal generation, and expected artifact identity. Execution must revalidate that authority and identity immediately before mutation and refuse if either changed. `ResolvePublication` remains categorically non-destructive.

That authorization layer can then be connected to the descriptor-pinned/hardened storage experiments without weakening the current publication-indeterminate safety rule.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
