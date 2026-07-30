# Experiment 0031: Complete Objects with Immutable Pages

- **Status:** Reproducible byte-level prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0015, 0029, and 0030
- **Script:** `tools/experiment_exp0002_immutable_page_objects.py`

## Question

Can immutable content-addressed directory pages authenticate complete object records, reuse unchanged historical objects, replace one object with bounded page copying, and reject object/structure overlap after all reachable outer hashes are recomputed?

## Prototype

The prototype combines:

- a fixed file header;
- 48-byte object headers followed by opaque payload bytes;
- domain-separated object digests over the complete object record;
- immutable 16 KiB directory pages from Experiment 0015;
- exact-end snapshots and commit footers;
- append-only replacement of one object identifier;
- strict validation of every active locator and referenced object.

The fixture contains 10,000 complete objects. Genesis sorts the object identifiers, appends all object records, emits the immutable directory, and publishes an exact-end snapshot. Replacement appends one new object record, copies one page per directory level, reuses all unaffected object records and pages, and publishes a linked snapshot.

## Strict checks

Validation requires:

- exact header, snapshot, footer, sequence, and parent linkage;
- current commit digest verification;
- authenticated root and page traversal;
- unique ordered object identifiers and matching root range;
- locator/object identifier, kind, logical-length, record-length, and digest agreement;
- every object range before the active snapshot;
- no object overlap with an active page, snapshot, or footer;
- no pair of active object records overlapping physically.

Historical objects and pages can lie before the current commit span. Their own authenticated digests remain required and are checked when reached from the active directory.

## Hostile cases

The executable evidence includes:

1. mutation of a reused historical object payload, which reaches and fails the object-digest check;
2. an authenticated leaf locator forged into the active snapshot range;
3. recomputation of the affected leaf digest, root digest, snapshot digest, and commit digest for that overlap case;
4. rejection at the physical object/structure overlap check rather than a shallow outer-digest mismatch;
5. interruption halfway through the latest footer, with the earlier complete genesis prefix remaining independently valid.

## Findings

1. Immutable directory pages can authenticate both newly appended and historical complete object records.
2. Replacing one existing object appends one object record and one new page per directory level while reusing unaffected bytes.
3. The current commit digest alone does not cover old reused objects; reachable object and page digests are therefore required.
4. Physical non-overlap remains an independent invariant even when every enclosing digest is internally consistent.
5. Exact-end publication preserves the previous complete prefix across interrupted replacement.

## Boundaries

This is not a complete successor epoch. It does not yet define roots, capabilities, extension placement, source-based bounded validation, recovery, insertion or deletion of complete objects, cross-language vectors, or a pinned invalid corpus. It uses the experimental 88-byte locator layout and 64-bit identifiers without selecting either.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_objects.py
```
