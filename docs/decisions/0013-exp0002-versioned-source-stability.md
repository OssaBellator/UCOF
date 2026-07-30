# ADR-0013: Require Versioned Stable Views for Mutable EXP-0002 Sources

- **Status:** Accepted
- **Date:** 2026-07-30
- **Scope:** Non-normative reference implementation and Phase 3 evidence
- **Related:** FCP-0002, Experiment 0012, threat model malicious-storage boundary

## Context

Candidate 1 readers perform multiple bounded range reads. A local immutable file normally presents one stable byte sequence for an operation, but remote and mutable storage can return bytes from different object versions across requests.

Digest checks may eventually detect many mixed-version combinations, but relying on accidental digest failure is insufficient:

- a current footer can be combined with stale ranges;
- a source can change between length discovery and footer retrieval;
- retry logic can silently mix versions;
- a valid old page or object can be served under a newer request;
- error classification and work accounting can become inconsistent.

The file format cannot manufacture external source stability. Storage-specific evidence such as object generations, immutable version IDs, or strong ETags exists outside Candidate 1 bytes.

## Decision

The reference experiment defines:

- `Exp0002SourceVersion`, an opaque 32-byte caller-produced version token;
- `Exp0002VersionedReadAt`, a range source that can report that token;
- `Exp0002StableSource`, an adapter that captures one expected token and requires it before and after every length or range read.

Any token change fails the operation with an I/O error. The strict validator, targeted lookup, and recovery scanner can operate through the adapter without changing their serialized-byte semantics.

The token is not a UCOF digest or registry value. The storage adapter is responsible for mapping strong storage evidence into the token. Weak validators, timestamps with insufficient resolution, or mutable object names without generation evidence are not acceptable merely because they can be hashed into 32 bytes.

## Consequences

### Positive

- mixed-version range reads fail closed;
- the validation and lookup APIs remain transport-neutral;
- storage-specific version syntax does not enter Candidate 1 bytes;
- the same adapter can protect local mutable files, cloud objects, and HTTP range clients;
- tests can deterministically simulate a token change even when returned bytes are unchanged.

### Costs and limits

- every read requires version evidence before and after the byte request;
- some transports may need conditional requests or extra metadata calls;
- the adapter proves one observed stable version, not that the version is the newest trusted version;
- a malicious source that lies consistently about its token is still handled only by UCOF integrity checks and external trust policy;
- the current synchronous trait does not define retries, cancellation, deadlines, or asynchronous request coalescing.

## Storage guidance

A concrete transport should prefer atomic conditional range requests tied to the expected version. When only separate version checks are possible, the before-and-after adapter is a fail-closed baseline but may have a time-of-check/time-of-use window inside the storage system.

Examples of potentially suitable evidence include:

- an immutable cloud-object generation number and bucket/object identity;
- a strong HTTP ETag with `If-Match` on every range request;
- a content-addressed immutable object identifier;
- an application snapshot handle that guarantees immutable reads.

The adapter token must bind all identity components needed to prevent one object's version value from being confused with another object.

## Freshness boundary

A stable view is not trusted freshness. An attacker can consistently serve an older valid object and its matching version token. Detecting whole-file rollback requires external trusted state, a signed transparency mechanism, or another monotonic authority outside Candidate 1 integrity.

## Alternatives considered

### Rely only on commit and object digests

Rejected. Digests authenticate stored relationships but do not define how multi-request clients avoid mixing source versions or retry safely.

### Put a remote version token in the UCOF footer

Rejected for Candidate 1. Storage version syntax is transport-specific and often changes when the same bytes are copied. Binding it into canonical file identity would harm portability and deterministic reproduction.

### Read the whole remote object in one request

Rejected as a general requirement. It defeats bounded partial access and is infeasible for large archives.

### Check a version token only once

Rejected. A source may change after the initial check. The adapter checks before and after each operation.

## Validation

Unit tests require:

- a stable token to permit full source validation;
- a token change during a range read to fail validation;
- a token change before a read to fail without consuming bytes.

Experiment 0012 separately measures range work over an immutable local HTTP server.
