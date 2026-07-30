# Phase 3 Successor Evidence — Immutable Pages and Bounded Writers

**Status:** Experimental evidence; not an independently implementable epoch  
**Date:** 2026-07-30  
**Related proposal:** FCP-0002  
**Predecessor:** `UCOF-EXP-0002` Candidate 1

## Purpose

Candidate 1 remains the complete executable corpus for authenticated paged directories, snapshots, source validation, recovery, history, repair, and rewrite. Experiment 0011 proved that its page-sequence equality prevents exact historical directory-page reuse.

This appendix consolidates the successor-design work performed after that negative result. The experiments below are microformats and models. They do not silently redefine Candidate 1 and do not constitute Candidate 2.

## Page identity

### Experiment 0014

Three page identity alternatives were compared:

| Alternative | Exact reuse | Identical-content deduplication | Direct page-age binding | External freshness |
|---|---|---|---|---|
| Active snapshot sequence | No | No | Yes | No |
| Page birth sequence | Yes | No | Yes | No |
| Immutable content identity | Yes | Yes | No | No |

At 100 million objects, Candidate 1 rewrites 542,671 pages or 8,891,121,664 bytes for one changed object. A no-split immutable tree rewrites four pages or 65,536 bytes, a 135,667.75x directory-byte difference.

**Current experimental direction:** immutable content identity. Membership remains authenticated through parent/root page digests and the publishing snapshot. Page age is not freshness and does not prevent whole-file replay.

## Byte-level copy-on-write

### Experiment 0015

A 100,000-object immutable-page byte microformat demonstrates:

- exact reuse of unchanged historical pages;
- strict traversal of mixed-age pages;
- one no-split replacement emitting three pages and reusing 542 of 545;
- a two-leaf batch emitting five pages independent of update order;
- mutation of a reused page failing at its page digest;
- interrupted latest footer rejection with the earlier exact prefix remaining valid.

This proves that active snapshot sequence is unnecessary for page membership. It does not yet cover insertion, deletion, split, merge, complete object records, capabilities, or cross-language vectors.

## Structural updates

### Experiment 0023

The immutable-page microformat now exercises:

- insertion into a full leaf;
- deterministic leaf split;
- deletion and deterministic sibling merge or redistribution;
- root-height increase;
- root-height collapse;
- duplicate insertion rejection;
- exact reuse of unaffected sibling pages.

The current deletion prototype supports leaf roots and height-one internal roots. Recursive internal rebalancing remains pending.

### Experiment 0024

A fixed-seed, 512-operation differential sequence compares the byte tree with an independent sorted-set oracle after every insert or delete. It also tests deterministic replay, bounded page emission, reuse observations, and root-height transitions.

Sparse child min/max ranges require a canonical insertion-routing rule. The selected prototype rule routes to the first child whose maximum is at least the new identifier, or to the final child when no maximum qualifies.

## Bounded deterministic writer

### Experiment 0013

A bounded external sorter processes 200,003 exact 88-byte locator-shaped records with sub-megabyte in-memory runs, deterministic output across run sizes, complete-key checks, and duplicate rejection.

### Experiment 0016

The sorted stream feeds canonical immutable page emission directly:

| Entries/run | Runs | Peak sort bytes | Locator spill | Reference spill | Pages | Output bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 4,096 | 49 | 360,448 | 17,600,264 | 69,632 | 1,088 | 17,825,792 |
| 7,777 | 26 | 684,376 | 17,600,264 | 69,632 | 1,088 | 17,825,792 |

Both spill configurations and a directly sorted baseline produce identical page bytes, root digest, and whole-output SHA-256. Complete page-reference levels are spilled as fixed 64-byte records rather than retained in memory.

Production work remains:

- staged merge with an explicit open-run limit;
- private spill creation and permissions;
- disk, inode, descriptor, merge-pass, and total-I/O budgets;
- cancellation and crash cleanup;
- metadata confidentiality policy;
- durable snapshot and footer publication.

## Locator layout

### Experiments 0010 and 0017

At 100 million objects with 16 KiB pages:

| Leaf layout | Directory size |
|---|---:|
| 88-byte Candidate 1, 64-bit ID | 8.280 GiB |
| 72-byte tight mirrored, 64-bit ID | 6.778 GiB |
| 56-byte minimal authenticated, 64-bit ID | 5.264 GiB |
| 64-byte minimal authenticated, 128-bit ID | 6.007 GiB |

The 56-byte locator transfers fewer total bytes than the tight 72-byte mirrored locator only below approximately 33.9% metadata-inventory coverage. Above that threshold, required 48-byte object-header reads outweigh the directory saving and may add one range request per object.

Conclusions:

- retire Candidate 1's 16-byte per-leaf reserve for a successor unless a concrete use is justified;
- decide identifier width separately from mirrored metadata;
- weight sparse lookup and broad inventory explicitly;
- consider an optional inventory structure rather than forcing one primary locator to optimize both workloads.

## Source and transport semantics

### ADR-0013 and Experiment 0018

One assurance operation uses one strong expected source-version token. Conditional retries are permitted only against that same token. Partial responses are discarded and charged. Version mismatch, cancellation, deadline, and retry exhaustion terminate the operation. A new token requires a clean restart with fresh parser, digest, traversal, diagnostic, and output state.

Stable source view prevents mixed-version reads. It does not prove current freshness.

## Resource limits

### Experiment 0019

Candidate 1 defaults are independent implementation safety ceilings, not one jointly satisfiable conformance profile.

The clearest conflict is:

- `max_objects = 10,000,000`;
- `max_read_operations = 1,000,000`.

Even an unrealistically optimistic one-read-per-object full validation requires ten times the configured operation budget.

A future conformance profile must choose file, object, page, read, hash, allocation, recovery, and output minima jointly and publish boundary tests. Resource-policy refusal remains distinct from malformed-file rejection.

## Extension preservation

### Experiment 0020

A canonical length-delimited extension block demonstrates:

- sorted unique tags;
- per-record required criticality;
- zero padding and exact lengths;
- opaque byte-identical preservation of unknown optional records;
- rejection of unknown required records, duplicates, unordered tags, bad flags, bad padding, truncation, trailing bytes, and excess work.

Reserved-zero bytes are not an unknown-field preservation mechanism. Repair, rewrite, and compaction require an explicit preservation policy and must not silently drop unknown optional data.

## Semantic compaction inputs

### Experiment 0021

Semantic compaction requires both:

- a snapshot-retention policy such as active-only, last N verified sequences, and pinned identities;
- a profile or application dependency resolver for every retained object kind.

Unknown dependency semantics must abort or invoke an explicit conservative retain-all-unknown policy. Cycles are handled with visited tracking; missing dependencies and snapshot, node, edge, and depth limits fail closed.

`repair-all`, caller-selected rewrite, and semantic compaction remain three distinct assurance claims.

## External freshness

### Experiment 0022

Internal hashes, sequence, parent links, and verified history cannot detect replacement with an older complete valid whole file.

- TOFU protects only after one trusted observation.
- Trusted state should include exact identity as well as ordering to detect same-sequence forks.
- Trusted-state updates need atomic application semantics.
- Multi-device state requires secure synchronization.
- Online transparency can detect rollback and forks but introduces availability, privacy, witness, and proof-policy dependencies.

Phase 3 exposes snapshot identity, commit identity, sequence, roots, and verified history. It makes no freshness claim.

## Evidence status

Green read-only workflows exist for:

- page identity alternatives;
- immutable-page COW;
- immutable-page splits and merges;
- mixed operation sequences;
- spill-backed page emission;
- locator inventory crossover;
- stable-source retries;
- limit interactions;
- extension preservation;
- profile retention;
- external freshness models.

These workflows supplement, rather than replace, Candidate 1 Rust, Python, invalid-vector, portability, and fuzz evidence.

## Successor blockers

Before an immutable-page successor can be independently implemented, it still needs:

1. a complete byte specification with file, object, page, extension, snapshot, and footer structures;
2. deterministic recursive internal insertion, deletion, split, merge, and root-height algorithms;
3. one-transaction batching across replacements, insertions, and deletions;
4. object and structural physical-overlap rules;
5. roots, capabilities, unknown optional preservation, and required-feature semantics;
6. bounded source lookup, strict validation, recovery, and verified history;
7. staged spill merging and failure-safe durable publication;
8. jointly satisfiable support profiles and boundary vectors;
9. independent Rust/Python or separately maintained implementations;
10. valid, invalid, interrupted, fork, rollback-policy, and hostile-operation corpora;
11. fuzzing over arbitrary operation sequences and source failures;
12. maintainer review and explicit retirement or retention of Candidate 1.

## References

- `docs/experiments/0014-exp0002-page-identity-alternatives.md`
- `docs/experiments/0015-exp0002-immutable-page-cow.md`
- `docs/experiments/0016-exp0002-spill-page-emission.md`
- `docs/experiments/0017-exp0002-locator-inventory-crossover.md`
- `docs/experiments/0018-exp0002-stable-source-retries.md`
- `docs/experiments/0019-exp0002-limit-interactions.md`
- `docs/experiments/0020-exp0002-extension-preservation.md`
- `docs/experiments/0021-exp0002-profile-retention.md`
- `docs/experiments/0022-exp0002-freshness-models.md`
- `docs/experiments/0023-exp0002-immutable-page-splits.md`
- `docs/experiments/0024-exp0002-immutable-page-sequences.md`
- `docs/decisions/0013-exp0002-versioned-source-stability.md`
- `docs/decisions/0014-exp0002-restart-whole-operation-on-version-change.md`
