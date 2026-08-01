# Experiment 0065: canonical reference-list dependency profile

## Question

Can semantic compaction make a concrete dependency-completeness claim for one bounded, independently implementable payload profile without treating unknown object kinds as dependency-free?

## Profile

Two caller-selected non-zero kind values are configured:

- a leaf kind whose payload must be empty and has no dependencies;
- a reference kind whose payload is an eight-byte header followed by a canonical identifier list.

The reference payload contains a little-endian `u32` count, four zero bytes, and exactly that many strictly increasing non-zero little-endian `u64` identifiers. A separate per-object dependency limit applies before allocation. Every other kind returns `Unknown` and therefore remains subject to the caller's reject or retain-all policy.

## Result

The reusable resolver:

- rejects malformed headers, lengths, reserved bytes, zero identifiers, duplicate or unsorted identifiers, non-empty leaf payloads, and configured count overflow;
- drives the existing cycle-safe and limit-bounded semantic compactor;
- retains a four-object dependency closure from a five-object source and discards only the independent orphan;
- reports three visited edges and depth two for the pinned graph.

## Boundary

The kind values and payload profile are non-normative research evidence. This does not prove that any application profile has adopted the contract, does not preserve historical snapshots or signatures, and does not make unknown kinds safe to discard.
