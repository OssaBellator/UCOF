# ADR-0006: Separate strict diagnostics from non-conformance salvage

- **Status:** Accepted
- **Date:** 2026-07-30
- **Owners:** UCOF maintainers
- **Related FCPs:** FCP-0001
- **Supersedes:** None
- **Superseded by:** None

## Context

A hostile-input library needs useful failure information without turning recovery heuristics into alternate validity rules. A damaged file may still contain complete physical records, but finding those records does not establish an active root, directory authority, digest integrity, schema validity, or profile conformance.

Combining strict validation and salvage in one boolean API would encourage callers to treat "some data was recovered" as "the file is valid." It would also make error-count and read limits difficult to state precisely.

## Decision

The Rust reference implementation exposes two separate operations.

### Strict diagnosis

`DiagnosticValidator` runs bounded metadata inspection followed by bounded source validation.

It returns one of:

- `Verified`, only when the committed-prefix digest and all structural checks pass;
- `Invalid`, with bounded diagnostics and optional structurally inspected context.

A digest failure may retain an `InspectionReport`, but the overall status remains `Invalid` and no `SourceValidationReport` is returned.

### Prefix salvage

`PrefixSalvager` scans the bootstrap and complete, in-bounds record framing from offset zero. It reads record headers but does not read payload bodies, accept a directory, select a manifest, verify a digest, or establish an active commit.

Its status is always `UnverifiedPrefix`. The result type intentionally has no `valid` field.

Only records whose complete physical ranges are within the source are reported. Scanning stops at the first fatal framing error, truncation, configured record limit, or directory record.

### Shared bounds

Both operations use caller-provided `Limits`. Diagnostics are capped by `max_diagnostics`; source reads are capped by `max_total_bytes_read`; record count and declared payload lengths are also bounded.

## Consequences

### Positive

- recovered data cannot silently become conforming data;
- structural context can accompany an integrity failure without weakening status;
- salvage work is bounded and does not touch large payload bodies;
- applications can present useful recovery information while preserving a strict trust boundary.

### Negative

- callers must choose an operation and handle distinct result types;
- the initial salvage scanner stops at the first fatal error rather than resynchronizing on later magic values;
- EXP-0001 has no snapshot history, so salvage cannot select an older valid root.

## Security implications

- no salvage result may be used as signature, provenance, or integrity evidence;
- offsets in diagnostics are informational and must not be trusted as authoritative directory entries;
- future magic resynchronization, footer scanning, or root enumeration requires a separate bounded policy and Phase 3 review;
- diagnostic messages are implementation text, while `ErrorCategory` is the stable conceptual classification used by tests.

## Follow-up work

- add bounded resynchronization experiments in Phase 3;
- add valid-root enumeration after append-only snapshots exist;
- ensure repair writes a new file rather than mutating damaged input by default;
- fuzz strict diagnosis and prefix salvage independently.
