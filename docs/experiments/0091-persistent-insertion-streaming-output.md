# Experiment 0091: Persistent insertion streaming output

## Question

Can one persistent insertion preserve the existing deterministic split and root-growth algorithm while constructing only an absolute-offset append tail and copying the verified base through bounded sink writes?

## Construction

`append_persistent_insert_to`:

1. strictly validates the exact-end canonical base;
2. rejects invalid or duplicate object identities before output;
3. appends the inserted object record into a separate tail and assigns its absolute locator from `base_length + tail_length`;
4. follows the existing insertion path and emits replacement or split pages into the same tail;
5. preserves untouched page references exactly;
6. creates a new root when split propagation escapes the old root;
7. appends the linked snapshot and footer and computes the same commit digest as the owned insertion writer;
8. completes construction and limits before the first sink write;
9. copies the verified base and append tail through bounded sequential writes.

## Evidence

Rust tests compare the streaming and owned insertion writers across:

- insertion without a split;
- a full root leaf that splits and grows a new root;
- insertion into a multi-leaf internal tree;
- duplicate and invalid identities failing before output.

Every successful case requires byte-identical output, identical validated reports, identical page-write/reuse accounting, bounded sink requests, and a tail allocation smaller than the complete successor.

The `immutable_successor_persistent_insertion_streaming` fuzz target varies object counts across root-leaf and multi-leaf bases, insertion position, payload bytes and lengths, sink chunk sizes, and duplicate identifiers. Successful outputs must match the owned insertion writer exactly and pass canonical occupancy validation; duplicates must leave the sink untouched.

## Boundary

This accepts an in-memory verified base slice and owns the complete append tail. It handles one insertion only. Deletion-only and shared multi-`Put` tail output, bounded remote base copying, private staging, and durable publication remain separate. Sink failure after output begins is terminal, so atomic visibility still requires a publication protocol.
