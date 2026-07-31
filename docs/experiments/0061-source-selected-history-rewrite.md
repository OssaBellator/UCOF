# Experiment 0061 — Selected source history rewrite

**Status:** bounded-source historical retention evidence  
**Date:** 2026-07-31

## Question

Can a caller retain an explicit subset of strictly verified historical snapshot states from a random-access source without materializing the complete source file or falsely preserving original byte identities?

## Operation

The operation first revalidates the complete linked source history under one cumulative source budget. Selected source sequences are canonicalized in ascending order and must identify exact verified history entries.

For each selected prefix, the writer:

1. creates a bounded prefix view;
2. strictly validates and inventories its authenticated active tree;
3. compares the inventory identity with the earlier history report to detect a changed source view;
4. rereads and validates active object records;
5. retains only the current and immediately previous selected active states in memory.

The oldest selected state becomes a new genesis. Each later selected state is compared with the prior retained state to derive canonical insert, replace, and delete operations. Original sequence numbers and digests are reported as mappings to the new output commits.

When two selected snapshots have identical active semantic state, one unchanged object is reissued so the selected history boundary remains represented. This does not claim preservation of the source commit identity.

## Evidence

Integration tests cover:

- sparse selection from a four-entry source history;
- chronological canonicalization independent of caller order;
- singleton selection becoming a new genesis;
- distinct retention of two semantically identical selected states;
- missing and duplicate sequence rejection;
- one cumulative read-byte budget across history validation, per-prefix inventory, and object rereads.

## Assurance boundary

The operation preserves selected active semantic states, not source sequence numbers, offsets, page identities, snapshot digests, commit digests, provenance signatures, optional extensions, or timestamps. It currently materializes one selected snapshot's active payloads because the deterministic writer accepts owned inputs.

The input source must remain stable across the complete operation. A strong-version conditional adapter can supply that guarantee; ordinary random-access sources remain responsible for equivalent no-mixing behavior.
