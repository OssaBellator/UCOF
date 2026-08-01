# Experiment 0093: Persistent multi-Put streaming output

## Question

Can a shared batch of insertions and replacements preserve canonical grouping, shared-path rewriting, split propagation, and caller-order determinism while constructing only an absolute-offset append tail?

## Construction

`append_persistent_put_batch_to`:

1. strictly validates the exact-end canonical base;
2. rejects empty, duplicate, zero-identity, zero-kind, and over-limit batches before output;
3. canonicalizes input order by object identifier;
4. appends every replacement or insertion record into a separate tail and assigns absolute locators from `base_length + tail_length`;
5. routes sorted updates through shared leaf and internal paths;
6. merges each affected leaf once and applies the same canonical leaf/internal grouping as the owned multi-`Put` writer;
7. preserves untouched page references exactly and grows new root levels when split propagation requires them;
8. appends the linked snapshot and footer and computes the same commit digest as the owned writer;
9. distinguishes appended-page footer accounting from complete reachable-tree report accounting;
10. completes validation, construction, hashing, and limits before the first sink write;
11. copies the verified base and tail through bounded sequential writes.

## Evidence

Rust tests compare the streaming and owned multi-`Put` writers for:

- multiple insertions in one leaf;
- insertions in different leaves sharing one root rewrite;
- one insertion and one replacement in the same leaf;
- a leaf split caused by multiple insertions;
- simultaneous full-leaf splits that grow a new root;
- caller-order canonicalization;
- empty and duplicate batches failing before output.

Every successful case requires byte-identical output, identical validated reports, identical page-write/reuse accounting, bounded sink requests, and a tail allocation smaller than the complete successor.

The `immutable_successor_persistent_multi_put_streaming` fuzz target varies root-leaf and multi-leaf base sizes, two replacement locations, one or two insertions, replacement versus insertion composition, payload bytes and lengths, caller order, and sink chunk size. Successful output must match the owned writer exactly and pass canonical occupancy validation; duplicate batches must leave the sink untouched.

## Boundary

This accepts an in-memory verified base slice and owns the complete append tail. It closes tail separation for the current replacement-only, insertion-only, deletion-only, shared multi-`Put`, and canonical mixed paths, but does not provide one unified public dispatcher. Bounded remote base copying, private staging, durable publication, and proposed-epoch migration remain separate. Sink failure after output begins is terminal, so atomic visibility still requires a publication protocol.
