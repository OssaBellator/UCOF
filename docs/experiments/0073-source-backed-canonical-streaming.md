# Experiment 0073: source-backed canonical streaming output

## Question

Can canonical genesis bytes be emitted from bounded random-access payload sources without materializing every payload or the complete output file?

## Source contract

Each payload source exposes:

- object identifier and kind;
- logical payload length;
- a strong immutable-view version that must not be reused after content change;
- bounded exact reads by payload-relative offset.

The writer records every source's initial version before output, then checks the same version immediately before and after that source's payload reads.

## Writer

`write_genesis_sources_to`:

- validates and canonicalizes metadata before the first output write;
- preflights exact object, page, snapshot, footer, file, depth, and allocation limits;
- reads each payload through one caller-bounded reusable buffer;
- incrementally hashes object and commit identities;
- emits through the bounded canonical sink from Experiment 0072;
- reports source read operations, bytes read, version checks, largest source buffer, output requests, and canonical output identity.

## Evidence

The pinned 400-object case uses reversed source order, 31-byte source reads, and 113-byte sink writes. It matches the owned canonical writer byte-for-byte and passes strict canonical occupancy validation. Additional cases establish that:

- duplicate metadata fails before output;
- source read failure after output begins is terminal and returns no publication report;
- a version change during payload reads is terminal and returns no publication report.

## Boundary

The contract assumes a strong version with no ABA reuse. A source or version failure can occur after partial sequential output, so callers still require private staging for atomic visibility. Locator metadata remains proportional to object count. This API streams independent payload sources; adapting a validated historical file inventory into these sources remains separate integration work.
