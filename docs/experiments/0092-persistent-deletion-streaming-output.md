# Experiment 0092: Persistent deletion streaming output

## Question

Can one persistent deletion preserve deterministic borrow, merge, recursive repair, and root-collapse behavior while constructing only an absolute-offset append tail and copying the verified base through bounded sink writes?

## Construction

`append_persistent_delete_to`:

1. strictly validates the exact-end canonical base;
2. rejects zero, missing, and final-object deletion requests before output;
3. follows the existing deletion path and tracks every original page whose identity is consumed by repair;
4. applies left-first borrow, then right borrow, then merge-left or merge-right exactly as the owned writer;
5. emits every changed leaf or internal page into a separate tail with absolute base-plus-tail offsets;
6. collapses a one-child root without needlessly re-emitting it;
7. appends the linked snapshot and footer and computes the same commit digest as the owned deletion writer;
8. distinguishes appended-page footer accounting from complete reachable-tree report accounting;
9. completes validation, construction, hashing, and limits before the first sink write;
10. copies the verified base and tail through bounded sequential writes.

## Evidence

Rust tests compare the streaming and owned deletion writers for:

- root-leaf deletion;
- multi-leaf deletion without underflow;
- left-sibling borrowing;
- minimum-sibling merge and root collapse;
- zero, missing, and final-object requests failing before output.

Every successful case requires byte-identical output, identical validated reports, identical page-write/reuse accounting, bounded sink requests, and a tail allocation smaller than the complete successor.

The `immutable_successor_persistent_deletion_streaming` fuzz target varies active object counts, payload bytes and lengths, deletion position, sink chunk size, and missing identifiers. Successful outputs must match the owned deletion writer exactly and pass canonical occupancy validation; invalid requests must leave the sink untouched.

## Boundary

This accepts an in-memory verified base slice and owns the complete append tail. It handles one deletion only. Shared multi-`Put` tail output, bounded remote base copying, private staging, and durable publication remain separate. The inherited recursive level-two repair algorithm remains covered by the owned deletion tests and fuzzing; this experiment primarily proves that the same emitted pages and offsets survive tail separation. Sink failure after output begins is terminal, so atomic visibility still requires a publication protocol.
