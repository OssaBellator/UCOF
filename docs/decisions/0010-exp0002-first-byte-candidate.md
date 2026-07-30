# ADR-0010: Use fixed authenticated 16 KiB pages for the first EXP-0002 byte candidate

- **Status:** Accepted for experiment only
- **Date:** 2026-07-30
- **Owners:** UCOF maintainers
- **Related FCPs:** FCP-0002
- **Supersedes:** None
- **Superseded by:** None

## Context

The Phase 3 algorithm models demonstrate that an ordered paged directory can provide bounded lookup, ordered inventory, local corruption isolation, and low copy-on-write path amplification. They do not select serialized bytes.

A concrete disposable byte experiment is now needed to measure page parsing, authenticated lookup, snapshot publication, previous-root recovery, cross-language reproduction, and interruption behaviour. The first candidate should be rigid enough to implement independently while leaving room for later experiments with page size and metadata encoding.

## Decision

The first `UCOF-EXP-0002` byte candidate uses:

- a 64-byte fixed file header;
- little-endian unsigned integers;
- 16 KiB fixed-size directory pages;
- fixed binary leaf and internal entries;
- SHA-256 with explicit domain tags for object-record, page, snapshot, and commit identities;
- leaf entries that carry the exact object-record offset, length, logical length, kind, and record digest;
- internal entries that carry an inclusive child key range, exact page offset, page length, and page digest;
- a variable-size snapshot record with sequence, parent snapshot identity, directory-root locator and digest, roots, and capability identifiers;
- a fixed 160-byte footer containing commit range, snapshot locator and digest, sequence, previous-footer offset, record count, and commit digest;
- an exact-end strict mode;
- recovery that is separately requested and bounded;
- `u64::MAX` as the genesis sentinel for absent previous-footer and parent offsets.

The commit digest is domain separated and binds the footer's semantic fields together with the exact commit bytes before the footer. Footer magic, fixed length, and zero-reserved bytes receive separate structural checks.

The snapshot digest authenticates the complete snapshot record. The directory-root digest authenticates the root page; every internal page authenticates its children; every leaf entry authenticates its referenced object record. This permits a new snapshot to reuse older object records without requiring the latest commit digest to cover all historical bytes.

The initial writer rebuilds the complete directory for every snapshot. Copy-on-write page reuse remains a separate experiment after the identity and recovery rules are executable.

## Candidate dimensions

### Directory page

- exact total length: 16,384 bytes;
- common page header: 64 bytes;
- leaf entry: 88 bytes;
- internal entry: 64 bytes;
- maximum leaf entries: 185;
- maximum internal entries: 255;
- unused bytes must be zero.

### Object record

- fixed 48-byte header;
- no transforms or record metadata in the first candidate;
- payload length is known before writing;
- record identity covers the exact header and payload bytes.

### Snapshot

- fixed 160-byte header followed by packed `u64` arrays;
- complete checkpoints only in the first byte implementation;
- root, required-capability, and optional-capability arrays have explicit counts and checked lengths;
- parent snapshot identity and previous-footer locator are both present but serve different purposes.

### Footer

- exact length: 160 bytes;
- footer semantic fields are bound by the commit digest;
- normal validation reads one exact-end footer only;
- previous-footer traversal and backward scanning are recovery operations.

## Decision drivers

- page-local parsing without a general metadata decoder;
- deterministic cross-language reproduction;
- a small fixed set of record and page kinds;
- authenticated root-to-leaf lookup;
- explicit publication and previous-root recovery;
- checked arithmetic and bounded allocation;
- enough structure to measure before generalizing.

## Alternatives retained for comparison

- 4 KiB pages;
- 64 KiB pages;
- restricted canonical-CBOR page entries;
- chunked sorted arrays;
- deterministic hash pages;
- a smaller footer with an external commit descriptor;
- a front-of-file alternating superblock as an optional local-filesystem optimization.

## Consequences

### Positive

- a lookup can authenticate one root-to-leaf path and one object record;
- page ranges and fixed capacities are straightforward to validate;
- previous complete footers can be traversed without scanning when one valid footer is known;
- cross-language implementations can reproduce exact bytes without schema-library dependencies;
- old object records can be safely referenced through authenticated leaf entries.

### Negative

- 16 KiB pages may waste space for very small directories;
- fixed entries reserve fields before transform and schema semantics exist;
- rebuilding the complete directory per snapshot is expensive;
- fixed binary metadata is less extensible than canonical maps;
- the footer and snapshot are relatively large;
- this candidate may be retired after measurements.

## Security implications

- every offset and length must be checked before range construction;
- page entry counts must fit the fixed page capacity;
- unused page and reserved bytes must be zero;
- object, page, snapshot, and commit digest domains must not overlap;
- digest algorithm identity must be explicit and included in the commit preimage;
- the previous-footer pointer must be less than the current footer offset or use the genesis sentinel;
- parent snapshot identity must agree with the snapshot reached through the previous-footer chain when history is retained;
- recovery candidate count, scan bytes, validations, chain depth, and results remain bounded independently;
- sequence is not an external freshness guarantee.

## Follow-up work

- publish the complete provisional byte specification;
- implement Rust and independent Python writers/readers;
- publish deterministic vectors for genesis, append, interrupted append, fork, invalid page, invalid object digest, and previous-pointer cycles;
- measure 4 KiB and 64 KiB alternatives;
- add concrete parser fuzz targets;
- update FCP-0002 with evidence and rejected alternatives;
- retire this byte candidate rather than preserving compatibility if core assumptions fail.
