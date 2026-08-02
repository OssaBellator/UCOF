# Experiment 0105: Selected-history owned-output cap

## Question

Can callers place an explicit upper bound on the complete selected-history output allocation while preserving fail-before-output behavior and existing byte semantics?

## Policy

`rewrite_versioned_source_selected_history_to_with_owned_output_cap` accepts the existing streaming options plus `max_owned_output_bytes`. The wrapper narrows only the output writer's `max_output_bytes` limit before invoking the established version-bound selected-history rewriter.

The source file, source history, read-operation, read-byte, and request-size limits are unchanged. If the reissued history cannot fit within the caller's owned-output cap, construction fails before the first sink write. An exact cap preserves the existing canonical bytes, retained source/output mappings, version binding, source statistics, and bounded sink writes.

## Evidence

Deterministic tests cover:

- an exact output cap producing byte-identical output and the same retained mappings as the owned rewriter;
- an undersized cap failing before any sink byte is written;
- preservation of the caller's maximum write-request bound.

## Boundary

This does not make multi-commit history output constant-memory. The complete reissued chain is still owned before sink copying, now under an explicit caller-selected bound. A later frontier must construct and validate chronological commit tails without retaining the complete chain. Provider adapters, retry/authentication integration, semantic selection independently per retained state, and private staged publication remain separate.
