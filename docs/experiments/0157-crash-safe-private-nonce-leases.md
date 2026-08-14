# Experiment 0157 — crash-safe private-stage nonce leases

**Status:** non-normative Phase 3 contract evidence; **durable commit remains an external boundary**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0155–0156

## Purpose

Experiment 0155 proves an operation-wide monotonic nonce counter contract, and Experiment 0156 carries the next nonce state in authenticated restart authority. A critical crash gap remains if the writer records nonce progress only after ciphertext is written: a crash can restore an older counter and reuse a nonce under the same key/prefix.

This experiment replaces post-use checkpointing with a nonce-lease/high-water-mark contract.

A bounded range of counters is reserved first. The reservation's high-water mark must be durably committed before any counter in that range becomes issuable. On restart, the writer resumes from the committed high-water mark and intentionally abandons every unused counter in the old lease.

Wasting nonces is acceptable. Reusing one is not.

This model does not implement filesystem durability. The `durably_committed` transition is the explicit boundary that a future authenticated journal backend must satisfy.

## Durable nonce state

The model keeps:

- journal/reservation generation;
- `next_unreserved: Option<u64>`.

`Some(N)` means counter `N` is the first counter not already covered by a durably committed reservation.

`None` means the global counter space is exhausted.

## Pending reservation

`reserve_nonce_lease` creates a `PendingNonceLease` containing:

- the exact durable state it was based on;
- the candidate committed durable state;
- the first counter in the lease;
- the last counter in the lease.

The pending type has no nonce-allocation method.

This is intentional: a merely planned or written-but-not-durable reservation must not make any nonce usable.

## Activation boundary

`activate_nonce_lease` requires both:

1. the current durable state still equals the reservation's base state;
2. the caller asserts that the reservation high-water mark has been durably committed.

Only then does it return an `ActiveNonceLease` with an allocation method.

If another reservation advanced durable state first, activation fails as `StaleReservation`.

If durable commit has not occurred, activation fails as `NotDurablyCommitted`.

## Crash before durable commit

A pending reservation may be lost in a crash.

Because pending state cannot issue a nonce, restart may safely load the previous durable state and reserve the same numeric range again. No nonce reuse occurs because none from the abandoned pending range were ever issuable.

A regression reserves counters 0–3, crashes before durable activation, restarts from counter 0, reserves 0–3 again, durably activates the second reservation, and then issues counter 0 for the first time.

## Crash after durable commit

Once the high-water mark is durably committed, restart must honor the entire reservation whether or not its nonces were used.

The main crash-cut regression uses a four-counter lease `[0, 3]` and evaluates every number of pre-crash uses from 0 through 4.

For every cut:

- the durable state after lease commit is `next_unreserved = 4`;
- restart begins from that durable state;
- the next lease is `[4, 7]`;
- every post-restart counter is at least 4;
- no pre-crash issued counter appears after restart.

Therefore even a crash immediately after durable reservation and before the first nonce use burns the entire old range.

## Bounded lease size

Lease size is validated before state advances.

The model rejects:

- zero lease size;
- zero maximum lease size;
- a lease larger than the configured maximum.

This prevents a single reservation from unnecessarily burning an unbounded portion of nonce space and provides a future performance/durability tuning knob.

## Sequential lease campaign

A regression commits 1,000 sequential leases of 17 counters each.

It requires:

- lease `i` to cover exactly `[17*i, 17*i + 16]`;
- all 17 counters to be issued in order;
- the lease to become terminally exhausted after its final counter;
- durable generation to advance once per committed lease;
- final `next_unreserved = 17,000`.

The campaign demonstrates disjoint monotonic reservation without retaining a global counter set.

## Stale reservation race

Two pending leases can be planned from the same durable base in the model.

If one is durably committed first, the other can no longer activate because its base state is stale.

A regression reserves `[0, 3]` and `[0, 7]` from generation zero, commits the second reservation, then requires the first activation to fail as `StaleReservation`.

A real journal backend should achieve the equivalent property through exclusive operation ownership, generation compare-and-swap semantics, or another single-writer mechanism.

## Counter exhaustion

Counter arithmetic is checked.

A boundary regression begins at `u64::MAX - 1`, reserves exactly two counters, and requires:

- lease first = `u64::MAX - 1`;
- lease last = `u64::MAX`;
- committed `next_unreserved = None`;
- both final counters issuable exactly once;
- the active lease then exhausted;
- no new global lease reservable afterward.

A second regression begins at `u64::MAX` and proves that reserving two counters fails with `CounterOverflow`, while reserving the single final counter succeeds and transitions durable state to exhausted.

No wrap to zero is accepted.

## Generation exhaustion

Reservation also checked-adds the durable generation.

A state already at `u64::MAX` generation cannot create another pending lease and fails as `GenerationExhausted` before activation.

## What this closes

The model closes the logical crash-safety gap identified by Experiment 0156:

- no nonce is issuable before a high-water reservation is durable;
- crash after durable reservation cannot cause a leased counter to be reused;
- unused counters from a committed lease are deliberately abandoned;
- stale reservations cannot activate after durable state advances;
- leases are size-bounded and disjoint;
- global counter exhaustion does not wrap.

## What remains open

The experiment does **not** prove real crash durability. Still required are:

- an authenticated journal backend that durably commits the lease high-water mark before activation;
- crash-consistent replacement/sync ordering for that journal and its parent directory;
- a trusted generation/rollback anchor;
- integration between the journal's nonce high-water mark and real AEAD sealing;
- proof that no code path can bypass the lease allocator and construct a nonce independently;
- operation-key and nonce-prefix provenance across process restarts;
- physical power-loss/filesystem qualification.

The safest production interpretation is conservative: after restart, never recover or reuse counters from a previously committed lease, even if evidence suggests they were unused.

## Verification

Implementation head `335c4e8f7a331eafecd5a81780be65a6a4e32802` is green on the decisive gates:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including every crash-cut, stale-reservation, bounded-lease, sequential-lease, exhaustion, and generation-overflow case;
- concrete HTTP/source tests;
- policy, parser, invalid-corpus, vector, and EXP-0003 scaffold/amendment verification;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The standard framing replay was still completing when this note was written; it does not alter the nonce-reuse result.

## Next executable slice

The next independent cleanup slice should keep journal authority as a hard safety input. Stale-operation discovery must be bounded by scan-entry and scan-byte limits, cleanup by entry/byte limits, and quarantine by its own cap. Authenticated `ResolvePublication` state must never become an automatic deletion action, regardless of age.

After that planner is green, the higher-value integration step is to combine the lease model with the authenticated journal contract so a lease reservation is represented as a new authenticated journal generation and cannot become active until the backend reports that generation durably committed.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.