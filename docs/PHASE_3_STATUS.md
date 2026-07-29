# Phase 3 Status — Directory, Snapshots, and Recovery

**Status:** In progress; proposal and algorithm-model increment implemented  
**Started:** 2026-07-30  
**Working branch:** `phase-3/directory-snapshots-recovery`  
**Depends on:** Phase 2 pull request #2

## Objective

Deliver the access and durability properties that distinguish UCOF from a simple chunked archive while preserving the rule that damaged or recovered state never silently becomes valid state.

## Initial decisions

- Phase 3 requires a new disposable epoch because directory and active-root semantics change validity.
- The draft target is `UCOF-EXP-0002`.
- Strict validation remains exact-end.
- Recovery scanning is explicit and separately bounded.
- A flat directory is not a candidate for promotion; its UC-02 failure is already measured.
- Research algorithms live in `ucof-experiments`, not the normative core.
- Complete checkpoints are prioritized before progress checkpoints.
- Repair and compaction write new output by default.

## Implemented first increment

| Frontier | Status | Evidence |
|---|---|---|
| EXP-0002 scope and invariants | Drafted | FCP-0002 |
| New-epoch requirement | Drafted | FCP-0002 compatibility section |
| Paged primary-directory model | Implemented | `ucof-experiments::directory` |
| Sorted leaf construction | Implemented and tested | paged directory tests |
| Bounded root-to-leaf lookup | Implemented and tested | `PagedDirectory::lookup` |
| Page graph validation | Implemented and tested | range, overlap, reference, and cycle checks |
| Closed-form massive-directory estimates | Implemented and tested | `estimate_shape` |
| Exact-end root-selection model | Implemented and tested | `RootSelectionMode::StrictExactEnd` |
| Recovery root-selection model | Implemented and tested | `RootSelectionMode::Recovery` |
| Parent-chain validation | Implemented and tested | sequence, gap, missing parent, cycle, depth checks |
| Fork ambiguity handling | Implemented and tested | equal-priority fork rejection |
| Progress checkpoint exclusion | Implemented and tested | `CheckpointKind::Progress` |
| Invalid candidate isolation | Implemented and tested | rejected-candidate reporting |
| Compaction reachability model | Implemented and tested | `ObjectGraph::plan` |
| Cycle-safe iterative traversal | Implemented and tested | compaction cycle test |
| Orphan reporting | Implemented and tested | `CompactionPlan::orphaned` |
| Node, edge, and depth limits | Implemented and tested | independent compaction limits |
| Research-isolation decision | Accepted | ADR-0009 |

## Properties demonstrated

### Directory

- input entries are sorted canonically by object identifier;
- duplicate identifiers are rejected before pages are created;
- leaf keys and child ranges are ordered and non-overlapping;
- child claims are cross-checked against referenced page ranges;
- invalid references and page cycles fail closed;
- lookup stops after one root-to-leaf path;
- a 100,000-object constructed directory requires only a small number of page reads per lookup;
- a 100,000,000-entry shape can be estimated without allocating the entries;
- page-read limits stop traversal before unbounded work.

### Snapshots and recovery

- strict mode accepts only a verified complete exact-end candidate;
- an interrupted append causes strict rejection while recovery can select the earlier complete root;
- invalid high-sequence candidates do not block a lower valid chain;
- missing parents, non-increasing sequences, sequence gaps, cycles, and progress checkpoints are rejected;
- equal highest-sequence forks are ambiguous and never selected silently;
- candidate count and parent depth are bounded;
- snapshot sequence remains file-local ordering evidence rather than freshness proof.

### Compaction

- selected roots produce a deterministic reachable set;
- unreachable physical objects are reported as orphans;
- graph cycles terminate through visited-identity tracking;
- missing dependencies fail closed;
- node, edge, and dependency-depth limits are enforced independently;
- the model plans logical reachability only and does not claim to preserve byte-scoped signatures or commit identity.

## Current limitations

- no EXP-0002 byte specification exists yet;
- page size, entry representation, fanout, identity scope, footer size, and digest scope are unresolved;
- the ordered-page model is not yet compared with sorted-array and hash-page alternatives;
- model page identifiers are in-memory indexes, not file offsets;
- candidate parsing and cryptographic validation are assumed to have occurred before root selection;
- no backward scanner, forward commit enumerator, or checkpoint parser exists yet;
- no repair writer or compactor writes files yet;
- no append simulation has been run at every byte boundary;
- no independent implementation or byte vectors exist;
- progress checkpoints remain a research question;
- no freshness guarantee exists against replacement of the whole file with an older valid copy.

## Next frontier tasks

1. compile and correct the research models under stable, MSRV, 32-bit, and big-endian CI;
2. implement sorted-array and hash-page comparison models;
3. benchmark directory overhead, lookup reads, and append rewrite amplification;
4. define a candidate EXP-0002 bootstrap, record kinds, snapshot, page, and footer layout;
5. implement bounded footer candidate discovery and candidate-storm tests;
6. simulate interrupted append at every byte boundary around snapshot and footer publication;
7. implement valid-root enumeration with explicit candidate statuses;
8. implement repair-to-new-file and compaction-output experiments;
9. add fuzz targets for page validation, root selection, recovery scanning, and compaction;
10. create independent Python models and deterministic vectors;
11. update the threat model with Phase 3 findings;
12. resolve FCP-0002 open questions before moving it to Review.

## Exit rule

Phase 3 is not complete until a selected EXP-0002 layout provides bounded single-object lookup, append publication, previous-root recovery, unambiguous root selection, valid-root enumeration, repair, and compaction with cross-language vectors, hostile-input evidence, continuous fuzzing, and documented rejected alternatives.
