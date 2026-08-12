# Experiment 0080: versioned bounded source to canonical sink

## Question

Can one strongly versioned bounded random-access source be strictly inventoried and then reissued as canonical active-state genesis output without materializing the complete input or output, while preserving one cumulative source budget and rejecting mixed source views?

## API

`rewrite_versioned_source_to` composes the active inventory from Experiment 0079 with the bounded canonical sink:

- one `ImmutableVersionedReadAt` handle is used for inventory and payload emission;
- complete strict validation finishes before the first output byte;
- exact remaining active payload bytes and range-read operations are preflighted against the same cumulative source limits;
- the payload read chunk is bounded by both the source request limit and the output adapter's source-buffer limit;
- the strong source version is checked before output, before and after every active object, and after the final object;
- each streamed object digest must equal the authenticated inventory digest;
- object records, canonical pages, snapshot, footer, and commit digest are emitted through the bounded sequential sink;
- inactive historical object records are not reread by the output pass.

## Evidence

Pinned tests cover:

- a 400-object file with one historical replacement, producing bytes and reports equal to the existing owned active rewrite;
- 97-byte source request, 89-byte hash block, 31-byte active payload read, and 113-byte sink write bounds;
- exact cumulative inventory-plus-payload read accounting;
- 802 payload version checks for 400 active objects;
- an inactive 4,096-byte historical payload skipped while only the 12-byte active replacement and 17-byte second object are reread for output;
- deterministic cumulative-budget rejection before the first sink byte;
- terminal version change during active payload streaming after partial output;
- fuzzed object counts, source/hash/payload/sink chunk sizes, optional replacement, owned/source byte equality, exact payload-read accounting, request bounds, and forced mid-stream version changes.

## Boundary

The versioned source trait remains an assurance contract that concrete transport adapters must satisfy without ABA reuse. Source or sink failure after output begins is terminal, so atomic visibility still requires private staging and a separate publication protocol. This experiment reissues the latest active state only; selected historical states, provider retry classification, authentication refresh, and native asynchronous cancellation remain separate work.
