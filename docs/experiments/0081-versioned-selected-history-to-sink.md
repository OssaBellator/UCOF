# Experiment 0081: selected historical sequence from versioned source to sink

## Question

Can one exact historical sequence be selected from a strongly versioned bounded source and reissued as canonical genesis output without materializing the complete input or output, while preserving linked-history validation and one cumulative source budget?

## API

`rewrite_versioned_source_sequence_to`:

- records the source's strong non-ABA version;
- revalidates every linked exact prefix under the caller's source limits;
- rejects a version change during history validation before output;
- selects one history entry by sequence and derives its exact prefix length from the authenticated footer offset;
- applies the remaining cumulative source budget to that prefix;
- locks every prefix version check to the original source version;
- delegates strict prefix inventory and active-payload emission to `rewrite_versioned_source_to`;
- returns history statistics, selected prefix length, selected source/output reports, and cumulative source statistics.

The selected state is emitted as a new genesis file. It does not preserve historical-chain, offset, commit, extension, provenance, or signature identity.

## Evidence

Pinned tests cover:

- a three-commit source and selection of sequences 0, 1, and 2;
- byte and report equality with `rewrite_all` applied to each exact historical prefix;
- 31-byte source request, 29-byte hash block, 17-byte payload read, and 37-byte sink write bounds;
- selected prefix length equality with the authenticated history footer;
- missing sequence rejection before the first sink byte;
- version change during full history validation before the first sink byte;
- fuzzed one-to-eight objects, one-to-three commits, selected sequence, request/hash/payload/sink chunks, byte equivalence, missing sequences, and history-time version changes.

## Boundary

This experiment selects one exact sequence from the current linked history. It does not preserve the historical chain in the output, emit several selected snapshots, perform semantic dependency selection inside a historical state, or define provider retry/authentication behavior. Concrete adapters must satisfy the versioned source assurance contract, and atomic visibility still requires private staging and publication.
