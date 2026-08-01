# Experiment 0079: versioned bounded-source active inventory

## Question

Can the exact-end active state be strictly validated through bounded random-access reads while retaining authenticated object descriptors for later streaming, without materializing the complete file or accepting a mixed source version?

## Contract

`ImmutableVersionedReadAt` extends the bounded `ImmutableReadAt` contract with a strong view version. The version must identify one immutable view without ABA reuse and must bind source length and every successful range read for one assurance operation.

`inventory_source_at`:

- records the strong version before reading;
- performs the complete strict source validation path, including header, footer, commit linkage, commit digest, snapshot, authenticated page traversal, canonical ordering, object overlap checks, object records, and payload digests;
- retains active object identifier, kind, record offset, record length, logical length, and object digest;
- preserves cumulative read, hash, request, and allocation accounting;
- records the strong version again after validation;
- rejects the result if the version changed.

The returned inventory contains descriptors only. Payload bytes are not copied.

## Evidence

Pinned tests cover:

- a 400-object file with one historical replacement;
- equality with the strict active report;
- sorted authenticated active descriptors and the replacement's current kind and logical length;
- bounded 97-byte read requests and 89-byte hash blocks;
- payload-offset derivation from authenticated record offsets;
- terminal version change during validation;
- fuzzed object counts, request/hash block sizes, optional historical replacement, strict-report equivalence, ordered descriptors, request bounds, and forced mixed-version failure.

## Boundary

The trait is an assurance contract: transport adapters must prove that length and every accepted range are bound to the same non-ABA version. This experiment does not yet stream payload bytes from the inventory, define provider retry behavior, or integrate the conditional HTTP/cloud adapter chain. A later layer must preserve cumulative budgets across inventory and output and recheck the version while payloads are streamed.
