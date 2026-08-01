# Experiment 0095: Bounded verified persistent source copying

## Question

Can a previously constructed persistent append tail be combined with a random-access base source without retaining the complete base in memory, while preserving fail-before-output identity checks and bounded I/O requests?

## Construction

`append_verified_source_with_tail_to` accepts:

- an `ImmutableReadAt` source;
- an independently pinned whole-file length and SHA-256 identity;
- a previously constructed append tail;
- cumulative read-operation and byte budgets;
- bounded read, allocation, and write request sizes.

The executor:

1. validates the complete two-pass read budget before touching the source or sink;
2. checks the source length and hashes the entire source in bounded requests before the first sink write;
3. rejects a preflight identity mismatch with an untouched sink;
4. rereads and hashes the source while copying it through bounded sink writes;
5. rechecks length and digest after the copied base;
6. withholds the append tail when the second pass no longer matches the pinned identity;
7. writes the tail only after the copied base is revalidated;
8. reports cumulative reads, bytes, largest requests, base/tail bytes, and tail SHA-256.

## Evidence

Rust tests cover:

- exact equality with an owned persistent successor assembled from a real replacement tail;
- bounded source reads and sink writes;
- wrong pinned identity failing before output;
- mutation at the start of the second pass producing a terminal copy-phase mismatch and withholding the tail;
- insufficient two-pass budget failing before any source read or sink write;
- injected sink failure after output begins.

The `immutable_successor_persistent_source_copy` fuzz target varies arbitrary base and tail bytes, read and write chunk sizes, wrong identities, and second-pass mutation points. Stable sources must reproduce `base || tail` exactly. Preflight mismatches must leave the sink untouched, and copy-phase mismatches must never append the tail.

## Boundary

This is a bounded exact-copy executor, not a source-backed mutation planner. Tail construction still occurs in the existing in-memory persistent writers. `ImmutableReadAt` has no version token, so the executor detects a changed second pass but cannot promise that partial base bytes were never exposed before that terminal error. Atomic visibility requires private staging, and stable conditional providers remain a separate transport integration frontier.
