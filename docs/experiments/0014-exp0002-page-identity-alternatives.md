# Experiment 0014: EXP-0002 Page Identity Alternatives

- **Status:** Reproducible
- **Date:** 2026-07-30
- **Related:** FCP-0002, Experiment 0008, Experiment 0009, Experiment 0011
- **Script:** `tools/experiment_exp0002_page_identity_alternatives.py`

## Question

After Experiment 0011 proved that Candidate 1 snapshot-sequence equality prevents exact historical page reuse, which replacement page-identity semantics preserve authenticated lookup while permitting copy-on-write append?

## Compared alternatives

### Active snapshot sequence

Candidate 1 stores one `u64` sequence in every page and requires:

```text
page.sequence == active_snapshot.sequence
```

This directly binds a page to one snapshot sequence, but an unchanged historical page cannot be referenced by a later snapshot. Re-encoding the page changes its digest and every ancestor digest.

### Page birth sequence

A replacement could store the sequence at which the page was created and require:

```text
page.birth_sequence <= active_snapshot.sequence
```

This permits an old page to remain reachable from later snapshots. The birth value remains authenticated by the page digest.

However, identical page content created at different sequences has different bytes and digests. Birth binding therefore prevents content-level page deduplication across births and introduces an age field whose security value must be justified.

### Immutable content identity

A replacement can remove snapshot sequence from immutable page bytes, reserve the former field as zero, and authenticate membership through:

- the page digest;
- the authenticated parent entry or snapshot root;
- strict physical range and overlap checks;
- the publishing snapshot and exact-end commit.

Identical canonical page content then has identical bytes and digest regardless of the snapshot that first referenced it. This permits both historical reuse and content deduplication.

It does not directly encode page age. Page age is not external freshness and does not prevent whole-file rollback.

## Scale model

The script uses the actual Candidate 1 geometry:

- 16 KiB pages;
- 64-byte page header;
- 88-byte leaf entries;
- 64-byte internal entries;
- 100,000,000 objects.

The resulting tree has:

```text
leaves:          540,541
level-1 pages:     2,120
level-2 pages:         9
root pages:            1
total pages:     542,671
depth:                  4
```

A Candidate 1 append rewrites all 542,671 pages, or 8,891,121,664 bytes of directory metadata.

A no-split batched reusable tree rewrites only the unique pages on changed leaf paths. One changed object rewrites four pages, or 65,536 bytes. Candidate 1 therefore writes approximately 135,668 times as many directory bytes for that case.

The model also spreads batches deterministically across leaves and counts shared ancestors once. It intentionally excludes page splits, merges, and object-record bytes; those require a byte-level successor writer.

## Security comparison

| Alternative | Exact historical reuse | Identical-content dedup | Authenticated mutation detection | Direct page-age binding | External rollback protection |
|---|---|---|---|---|---|
| Active snapshot sequence | No | No | Yes | Yes | No |
| Page birth sequence | Yes | No | Yes | Yes | No |
| Immutable content identity | Yes | Yes | Yes | No | No |

All alternatives still require authenticated parent/root membership, strict range validation, canonical page parsing, and exact-end snapshot publication.

None of the alternatives prevents replay of an older valid whole file. External trusted freshness remains a separate trust-layer requirement.

## Findings

1. Candidate 1 active-sequence binding is incompatible with exact historical page reuse.
2. Page birth sequence permits reuse but makes otherwise identical pages differ by creation sequence.
3. Immutable content identity permits exact reuse and content deduplication; snapshot and root authentication already bind a page into the active directory.
4. A page-age field must not be mistaken for freshness or rollback protection.
5. Batched ancestor sharing is necessary; per-object path copying can still overproduce metadata.

## Current direction

Immutable content identity is the simplest baseline for the next disposable byte candidate because it:

- removes the confirmed reuse blocker;
- preserves deterministic canonical bytes;
- permits content deduplication;
- avoids assigning false security meaning to page age;
- leaves publication identity in snapshots and commits, where it is already authenticated.

This is an experimental direction, not an accepted FCP decision. A successor byte prototype must still prove:

- old and new page coexistence under exact physical-range checks;
- deterministic batched path copying and split behavior;
- valid root-to-leaf lookup across mixed-age pages;
- strict validation of reachable pages without accepting unreferenced bytes;
- interrupted append behavior;
- cross-language vectors and invalid cases;
- bounded writer, source-read, and recovery work.

## Reproduction

```console
python3 tools/experiment_exp0002_page_identity_alternatives.py
```
