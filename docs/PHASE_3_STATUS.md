# Phase 3 Status — Directory, Snapshots, and Recovery

**Status:** In progress; proposal and broad algorithm-model increment passing CI  
**Started:** 2026-07-30  
**Working branch:** `phase-3/directory-snapshots-recovery`  
**Stacked pull request:** #3  
**Depends on:** Phase 2 pull request #2

## Objective

Deliver the access and durability properties that distinguish UCOF from a simple chunked archive while preserving the rule that damaged, recovered, or merely plausible state never silently becomes valid state.

## Current decisions

- Phase 3 requires a new disposable epoch because directory and active-root semantics change validity.
- The draft target is `UCOF-EXP-0002`.
- No EXP-0002 bytes have been selected.
- Strict validation remains exact-end.
- Recovery scanning is explicit and separately bounded.
- A flat directory is not a promotion candidate; its UC-02 failure is already measured.
- An ordered paged tree is the current prototype baseline, not an accepted wire layout.
- Research algorithms live in private `ucof-experiments`, not the normative core.
- Complete and progress checkpoints have distinct authority.
- Repair and compaction operate only from verified complete snapshots and produce new identity.

## Implemented model frontiers

### Proposal and research boundaries

| Frontier | Status | Evidence |
|---|---|---|
| EXP-0002 scope and invariants | Drafted | FCP-0002 |
| New-epoch requirement | Drafted | FCP-0002 compatibility section |
| Strict versus recovery separation | Drafted and modelled | FCP-0002, recovery model |
| Non-normative research isolation | Accepted | ADR-0009 |
| Security findings companion | Published | `docs/security/EXP_0002_MODEL_FINDINGS.md` |

### Primary directory

| Frontier | Status | Evidence |
|---|---|---|
| Ordered paged-directory construction | Implemented and tested | `ucof-experiments::directory` |
| Canonical object-identifier ordering | Implemented and tested | leaf construction |
| Duplicate-key rejection | Implemented and tested | build tests |
| Bounded root-to-leaf lookup | Implemented and tested | `PagedDirectory::lookup` |
| Page range and reference validation | Implemented and tested | page validation |
| Overlap, forged range, and cycle rejection | Implemented and tested | corruption tests |
| Page-read limits | Implemented and tested | bounded lookup tests |
| Closed-form massive-directory estimates | Implemented and tested | `estimate_shape` |
| B+ tree, sorted-array, and hash-page comparison | Implemented | Experiment 0005 |

### Snapshots, enumeration, and recovery

| Frontier | Status | Evidence |
|---|---|---|
| Exact-end root selection | Implemented and tested | `RootSelectionMode::StrictExactEnd` |
| Recovery root selection | Implemented and tested | `RootSelectionMode::Recovery` |
| Parent-chain validation | Implemented and tested | sequence, gap, missing parent, cycle, depth checks |
| Equal-priority fork ambiguity | Implemented and tested | fork tests |
| Progress checkpoint exclusion | Implemented and tested | snapshot tests |
| Invalid candidate isolation | Implemented and tested | rejected-candidate reporting |
| Bounded valid-root enumeration | Implemented and tested | `RootEnumerationReport` |
| Ancestor, terminal, and fork statuses | Implemented and tested | enumeration tests |
| Bounded backward candidate scanner | Implemented and tested | `scan_backwards` |
| Independent scan, match, validation, and result limits | Implemented and tested | recovery scanner tests |
| Candidate-storm rejection | Implemented and tested | recovery scanner tests |
| Exhaustive interrupted append cuts | Implemented and tested | 8,192-cut recovery test |

### Publication and checkpoints

| Frontier | Status | Evidence |
|---|---|---|
| Ordered append publication state | Implemented and tested | `PublicationModel` |
| Footer-only main snapshot publication | Implemented and tested | publication tests |
| Interrupted-stage preservation of old root | Implemented and tested | publication tests |
| Complete checkpoint authority | Implemented and tested | checkpoint tests |
| Progress checkpoint non-authority | Implemented and tested | checkpoint tests |
| Stage regression and duplicate rejection | Implemented and tested | publication tests |
| Event and checkpoint limits | Implemented and tested | publication limits |

### Compaction and repair

| Frontier | Status | Evidence |
|---|---|---|
| Iterative compaction reachability | Implemented and tested | `ObjectGraph::plan` |
| Cycle-safe graph traversal | Implemented and tested | compaction cycle test |
| Missing-dependency rejection | Implemented and tested | compaction tests |
| Deterministic orphan reporting | Implemented and tested | `CompactionPlan` |
| Independent node, edge, and depth limits | Implemented and tested | compaction tests |
| Verified-source-only repair plan | Implemented and tested | `RepairPlan` |
| Duplicate, missing, overflow, and overlap locator rejection | Implemented and tested | repair tests |
| Copy-range and byte limits | Implemented and tested | repair limits |
| New snapshot identity requirement | Implemented | repair plan |
| Byte-scoped signature non-preservation | Explicit | repair plan |

## Independent evidence

The independent Python model validator uses JSON fixtures and imports no Rust implementation. It checks:

- directory shapes at 1,000, 1,000,000, and 100,000,000 entries;
- strict linear-chain selection;
- interrupted-append recovery;
- ambiguous forks;
- progress-checkpoint exclusion;
- reachability and orphan reporting;
- graph-cycle termination.

The directory comparison experiment evaluates a copy-on-write B+ tree, a monolithic sorted array, and deterministic hash pages. It currently selects the ordered paged tree as the strongest prototype baseline while retaining the others as measured alternatives.

## Continuous verification

The Phase 3 branch currently passes:

- locked dependency checks;
- stable Rust formatting and clippy with warnings denied;
- all workspace unit, integration, and documentation tests;
- the inherited independent EXP-0001 Python parser and adversarial corpus;
- the independent Phase 3 Python model corpus;
- all reproducible framing, footer, scale, and directory-model experiments;
- Rust 1.85 MSRV compilation;
- 32-bit `i686-unknown-linux-gnu` library compilation;
- big-endian `powerpc64-unknown-linux-gnu` library compilation;
- thirteen cargo-fuzz target builds and bounded pull-request smoke campaigns.

The Phase 3 fuzz targets cover:

1. paged directory construction, validation, and lookup;
2. snapshot selection;
3. root enumeration;
4. publication state transitions;
5. recovery candidate scanning;
6. compaction planning;
7. repair planning.

The six inherited Phase 2 targets continue covering full files, canonical metadata, metadata inspection, prefix salvage, sequential reading, and writer round trips.

## Properties demonstrated

### Directory

- input entries are sorted canonically by object identifier;
- duplicate identifiers are rejected before pages are created;
- leaf keys and child ranges are ordered and non-overlapping;
- child claims are cross-checked against referenced page ranges;
- invalid references, forged ranges, overlaps, and page cycles fail closed;
- lookup stops after one root-to-leaf path;
- a 100,000-object constructed directory requires only a small number of page reads per lookup;
- a 100,000,000-entry shape is estimated without allocating entries;
- page-read limits stop traversal before unbounded work.

### Snapshots and recovery

- strict mode accepts only a verified complete exact-end candidate;
- an interrupted append causes strict rejection while recovery can select an earlier complete root;
- invalid high-sequence candidates do not block a lower valid chain;
- missing parents, unverified parents, non-increasing sequences, gaps, cycles, depth exhaustion, and progress checkpoints are rejected;
- equal highest-sequence forks are marked ambiguous and never selected silently;
- root enumeration reports all bounded candidates without forcing selection;
- scan bytes, magic matches, candidate validations, results, candidates, and parent depth are independently bounded;
- snapshot sequence remains file-local ordering evidence rather than freshness proof.

### Publication

- only footer completion publishes the main snapshot;
- every pre-footer interruption preserves the previous complete snapshot;
- complete checkpoints can be independently readable and root eligible;
- progress checkpoints never become active roots;
- stage order, duplicate stages, event count, and checkpoint count are enforced.

### Compaction and repair

- selected roots produce deterministic reachable and orphan sets;
- graph cycles terminate through visited-identity tracking;
- missing dependencies fail closed;
- node, edge, dependency-depth, copy-range, and copy-byte limits are independent;
- repair rejects unverified or progress-checkpoint sources;
- reachable physical ranges must exist, be checked, and not overlap;
- repaired output requires a new snapshot identity and cannot falsely preserve byte-scoped signatures.

## Current limitations

- no EXP-0002 byte specification exists yet;
- page size, entry representation, fanout, physical locator, identity scope, footer size, and digest scope are unresolved;
- model page identifiers are in-memory indexes, not file offsets;
- candidate parsing and cryptographic validation are assumed before root selection and enumeration;
- no concrete forward commit parser or checkpoint parser exists;
- no repair writer or compactor writes EXP-0002 files;
- independent evidence covers algorithms, not serialized bytes;
- progress checkpoint bytes and long-running workload semantics remain unresolved;
- copy-on-write page reuse and streaming external sort are not implemented;
- no freshness guarantee exists against replacement of the whole file with an older valid copy;
- directory metadata confidentiality remains deferred to the encryption phase.

## Completed frontier tasks from the original Phase 3 list

- research models compile under stable, MSRV, 32-bit, and big-endian CI;
- sorted-array and hash-page alternatives have a reproducible comparison baseline;
- directory overhead, lookup reads, and update rewrite amplification have initial closed-form evidence;
- bounded footer candidate discovery and candidate-storm tests exist;
- interrupted append recovery is exercised at every cut in an 8,192-byte append;
- valid-root enumeration has explicit candidate statuses;
- verified repair and compaction planning models exist;
- page, selection, enumeration, publication, recovery, compaction, and repair fuzz targets run continuously;
- independent Python algorithm fixtures exist;
- a Phase 3 security findings companion is published.

## Next frontier tasks

1. resolve page size, entry encoding, fanout, identity, footer, digest, and previous-pointer questions;
2. define the first candidate EXP-0002 bootstrap, records, pages, snapshot, and footer bytes;
3. implement Rust and independent Python EXP-0002 byte writers and readers;
4. publish deterministic valid, invalid, interrupted-append, fork, checkpoint, repair, and compaction vectors;
5. compare 4 KiB, 16 KiB, and 64 KiB physical page implementations;
6. measure remote range requests, caching, and copy-on-write reuse with realistic traces;
7. implement authenticated candidate classification and concrete valid-root enumeration;
8. implement repair-to-new-file and compaction output for verified EXP-0002 snapshots;
9. fuzz concrete page, footer, snapshot, recovery, and compaction parsers;
10. integrate concrete-byte findings into the primary threat model;
11. resolve FCP-0002 open questions before moving it to Review.

## Exit rule

Phase 3 is not complete until a selected EXP-0002 layout provides bounded single-object lookup, append publication, previous-root recovery, unambiguous root selection, valid-root enumeration, repair, and compaction with cross-language byte vectors, hostile-input evidence, continuous fuzzing, and documented rejected alternatives.
