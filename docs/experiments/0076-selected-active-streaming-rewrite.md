# Experiment 0076: selected active-state streaming rewrite

## Question

Can caller-selected active objects be reissued through the bounded canonical sink without reading unselected active payloads, inactive historical payloads, or buffering the complete output?

## Selection

`rewrite_selected_active_file_to`:

- canonicalizes the requested identifier list before output;
- rejects empty, duplicate, over-limit, and missing selections before the first sink write;
- strictly validates the complete active source file;
- converts authenticated active locators into borrowed versioned payload sources;
- filters the source inventory in identifier order without cloning payloads;
- streams only retained payloads through the source-backed canonical writer.

## Evidence

Pinned cases cover:

- a 400-object file with a historical replacement, selecting identifiers in non-canonical caller order and producing bytes and reports equal to the existing `rewrite_selected` implementation;
- 31-byte payload reads and 113-byte sink writes;
- empty, duplicate, and missing selections leaving the sink untouched;
- selecting one 12-byte active replacement while skipping an inactive 4,096-byte historical payload and unselected 2,048-byte and 17-byte active payloads;
- fuzzed caller-order canonicalization, exact selected-read accounting, byte equivalence, request bounds, and untouched-sink duplicate/missing failures.

## Boundary

The result is a new genesis file and does not preserve historical, offset, snapshot, commit, extension, provenance, or signature identity. The adapter still begins from an in-memory source slice and retains locator metadata proportional to active object count. Dependency-aware semantic selection and selected historical-chain streaming remain separate work.
