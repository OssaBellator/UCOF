# Experiment 0089: Persistent Replacement Streaming Output

**Status:** Research evidence  
**Date:** 2026-08-01  
**Epoch allocation:** None

## Question

Can replacement-only persistent copy-on-write updates reuse the mixed writer's absolute-offset append-tail machinery and emit byte-identical successors without owning a second complete output file?

## Construction

`append_persistent_replacement_batch_to`:

1. strictly validates the exact-end canonical base;
2. canonicalizes and validates a non-empty replacement-only operation set;
3. appends replacement object records into a tail while computing absolute offsets as `base_length + tail_length`;
4. rewrites only affected leaf-to-root paths into the same tail;
5. preserves untouched page references exactly;
6. appends the linked snapshot and footer and hashes the exact commit bytes;
7. completes all validation, limit checks, construction, and hashing before the first sink write;
8. copies the verified base and append tail through bounded write requests.

The report uses the existing persistent streaming accounting: mode, page writes/reuse, base bytes, tail bytes, total bytes, largest write request, and tail allocation.

## Evidence

The Rust tests pin:

- exact byte and report equality with `append_persistent_batch` for replacements in three different leaves;
- exact page-write and page-reuse equality;
- caller-order-independent output;
- bounded sink requests;
- tail allocation smaller than the complete successor;
- deletion and insertion requests failing before output.

The `immutable_successor_persistent_replacement_streaming` fuzz target varies:

- two to 481 active objects;
- replacement locations across root-leaf and multi-leaf trees;
- replacement payload lengths and bytes;
- caller operation order;
- sink chunk size;
- invalid absent identifiers.

Every successful case is compared byte-for-byte and report-for-report with the owned persistent writer and strictly revalidated under canonical occupancy.

## Important boundary

This still accepts an in-memory base slice and owns the complete append tail. It does not yet cover insertion-only, deletion-only, or shared multi-`Put` streaming, bounded remote base copying, private staging, or durable publication. Sink failure after output begins is terminal; atomic visibility remains a separate publication concern.
