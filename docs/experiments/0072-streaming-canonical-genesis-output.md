# Experiment 0072: streaming canonical genesis output

## Question

Can the canonical successor genesis writer emit byte-identical output through a bounded sequential sink without materializing the complete file in one output buffer?

## Writer

`write_genesis_to`:

- validates non-empty object input, identifiers, kinds, duplicate identifiers, allocation limits, canonical tree depth, page count, file size, and exact output size before the first sink write;
- sorts only object indexes, leaving caller payloads owned by the caller;
- writes object headers and payloads separately;
- hashes object and commit identities incrementally;
- writes canonical half-full leaf and internal pages using the accepted research occupancy algorithm;
- chunks every sink request under a caller-selected maximum;
- emits the snapshot and footer without rereading earlier sink bytes;
- retains only ordered locator metadata and the current tree level rather than a complete output `Vec<u8>`.

## Evidence

Pinned tests compare output byte-for-byte with the canonical in-memory writer for:

- one object;
- one full leaf;
- the canonical 400-object multi-leaf tree.

The tests use reversed caller order and 113-byte sink requests, then strictly validate canonical occupancy. Additional tests establish that deterministic preflight failures leave the sink untouched, duplicate and invalid inputs fail before output, and injected I/O failure never returns a publication report or footer.

## Boundary

The function writes to a sequential sink and cannot roll back bytes after an I/O failure. Atomic visibility still requires private staging and an appropriate publication protocol. Input payloads remain caller-owned in memory, and locator metadata remains proportional to active object count; this removes whole-output buffering, not all object-count-dependent metadata. Source-backed payload streaming and selected historical rewrite integration remain separate work.
