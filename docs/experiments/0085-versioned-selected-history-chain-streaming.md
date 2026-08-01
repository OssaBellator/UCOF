# Experiment 0085: versioned selected-history chain streaming

## Question

Can multiple selected linked-history states from one non-ABA bounded source be reissued chronologically and copied through bounded sink writes without exposing output before all source validation and version checks complete?

## Construction

The experiment restores the selected-history rewriter on the versioned source/output lineage and adds `rewrite_versioned_source_selected_history_to`.

The source sequence list is canonicalized in ascending order. Complete linked-history validation and selected-prefix rereads occur under one cumulative source budget. The oldest selected state becomes output sequence zero; each later selected state becomes one subsequent commit. Semantic differences are converted to canonical mixed batch operations. Identical consecutive states preserve the selected boundary by reissuing one unchanged object. The complete output history is strictly validated before the source version is rechecked and before the first sink write.

After preflight, the complete reissued history is copied to the sink in bounded requests. Source failure, version change, and sink failure remain distinct. Sink failure after output begins is terminal and returns no success report.

## Evidence

Unit tests cover:

- caller-unsorted sequence selection and chronological canonicalization;
- exact byte and retained-report equality with the owned selected-history rewriter;
- one new output history entry per selected source sequence;
- bounded source reads and sink writes;
- missing-sequence failure before output;
- terminal source-version change before output.

The `immutable_successor_history_chain_to_sink` fuzz target varies object counts, one-to-three source commits, replacement locations, payloads, source and sink chunk sizes, and selected sequence order. It compares complete output bytes, retained mappings, source statistics, and output history length with the owned rewriter. Duplicate selections and forced source-version changes must leave the sink untouched.

## Boundary

This closes version-bound multi-snapshot reissuance semantics but still owns the complete output history before copying it to the sink. It therefore does not provide constant-memory multi-commit output. Historical byte identities, offsets, extensions, provenance, and signatures are not preserved. Concrete maintained provider adapters, retry/authentication integration, atomic private staging/publication, and semantic dependency selection independently per retained snapshot remain separate.
