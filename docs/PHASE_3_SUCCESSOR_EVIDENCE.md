# Phase 3 Successor Evidence — Immutable Pages and Bounded Writers

**Status:** Executable experimental evidence; not a proposed or independently implementable epoch  
**Date:** 2026-07-30  
**Related proposal:** FCP-0002  
**Predecessor:** `UCOF-EXP-0002` Candidate 1

## Purpose

Candidate 1 remains the complete disposable codec for authenticated paged directories, snapshots, bounded source access, recovery, history, repair, and caller-directed rewrite. Experiment 0011 proved that Candidate 1's page-sequence equality prevents exact historical directory-page reuse.

This appendix consolidates successor-design evidence produced after that negative result. The following algorithms, byte microformats, vectors, and transport models do not silently redefine Candidate 1 and do not constitute Candidate 2. They narrow the design space and expose the work still needed before another epoch can be proposed.

## Page identity

### Experiment 0014

Three page-identity alternatives were compared:

| Alternative | Exact reuse | Identical-content deduplication | Direct page-age binding | External freshness |
|---|---|---|---|---|
| Active snapshot sequence | No | No | Yes | No |
| Page birth sequence | Yes | No | Yes | No |
| Immutable content identity | Yes | Yes | No | No |

At 100 million objects, Candidate 1 rewrites 542,671 pages or 8,891,121,664 bytes for one changed object. A no-split immutable tree rewrites four pages or 65,536 bytes, a 135,667.75x directory-byte difference.

**Current successor direction:** immutable content identity. Membership is authenticated through child, root, and publishing-snapshot digests. Page identity does not establish age, authenticity, or external freshness.

## Byte-level copy-on-write

### Experiment 0015

A 100,000-object immutable-page byte microformat demonstrates:

- exact reuse of unchanged historical pages;
- strict traversal of mixed-age pages;
- one no-split replacement emitting three pages and reusing 542 of 545;
- a two-leaf batch emitting five pages independent of update order;
- mutation of a reused page failing at its page digest;
- interrupted latest-footer rejection while the previous exact prefix remains valid.

The two-leaf batch shares rewritten ancestors and emits only the final reachable pages. This establishes the intended batching property but is not yet a general mixed-operation transaction planner.

## Structural algorithms

### Experiments 0023 through 0027

The immutable-page microformat now executes:

- canonical insertion routing across sparse child ranges;
- deterministic insertion into non-full leaves;
- deterministic split of a full leaf;
- sibling merge and redistribution on deletion;
- root-height increase and collapse;
- recursive internal-node split propagation;
- recursive underflow propagation and internal deletion;
- exact reuse of unaffected sibling and cousin subtrees;
- exact resurrection of an earlier historical root when an inverse update restores identical contents;
- duplicate insertion and missing deletion rejection.

A new snapshot that reuses an old exact root still receives a new sequence, parent snapshot identity, snapshot digest, and commit digest. Structural root reuse is not publication-identity reuse.

### Experiments 0024 and 0030

Differential operation evidence includes:

- one fixed 512-operation sorted-set comparison;
- 34 deterministic seeds;
- 256 operations per seed;
- 8,704 insertions and deletions in total;
- deterministic replay of every seed;
- exact sorted-set agreement after every operation;
- page ordering, ranges, digests, and root-boundary checks;
- bounded per-operation page emission;
- root-height transition tracking.

The multi-seed campaign currently focuses on the constrained height-one operating envelope. Recursive split and delete boundaries are exercised separately rather than randomly interleaved at arbitrary depth.

## Complete objects and history

### Experiment 0031

The successor microformat integrates real 48-byte object headers and payloads. Strict validation:

- parses and cross-checks object identifier, kind, payload length, logical length, and locator claims;
- recomputes domain-separated object digests;
- rejects object/object and object/page/snapshot/footer overlap;
- detects mutation of reused historical object records;
- supports deterministic object replacement by appending one new object plus one page per rewritten tree level;
- rejects interrupted latest publication while the previous exact prefix remains valid.

A forged locator into an active structural page is reauthenticated through its leaf, root, snapshot, and commit. Validation still rejects it at the physical-overlap layer.

### Experiment 0032

Complete-object insertion and deletion demonstrate intentionally distinct assurance scopes:

- active validation authenticates only objects reachable from the active snapshot;
- verified history validates each linked ancestor as an independent strict prefix;
- corruption of a deleted historical object can leave the active snapshot valid while verified history rejects the ancestor that still references it;
- deterministic insert/delete replay produces identical output;
- sequences and object counts are reported for every verified prefix.

No active-validity API silently upgrades historical or recovered state.

## Bounded source access

### Experiment 0033

A synchronous random-access source prototype separates:

- targeted authenticated lookup;
- full exact-end source validation.

Both modes bound request size, operations, bytes read, pages, objects, hash work, and allocation. Commit and object digests stream in bounded chunks.

Targeted lookup authenticates the current commit, snapshot, one directory path, and the selected object or absence proof. Full validation walks every reachable page and hashes every active object.

A fixture containing an unrelated 1 MiB object proves that targeted lookup does not request that payload while full validation does.

The model uses a stable in-memory source. Production conditional HTTP/cloud adapters and asynchronous cancellation remain separate transport work.

## Roots, capabilities, and extensions

### Experiment 0034

An authenticated catalog object carries:

- sorted root object identifiers;
- sorted capability declarations;
- per-capability required criticality;
- canonical extension records;
- unknown optional extension bytes preserved exactly.

Unknown required capabilities remain visible after structural and cryptographic validation but prevent semantic interpretation. Missing roots, duplicate or zero roots, malformed capability ordering, unsupported flags, malformed extension bytes, and catalog work-limit violations fail closed.

This demonstrates one implementable placement strategy. It does not allocate normative capability identifiers or prove that one catalog object is the best final layout.

## Recovery and verified history

### Experiment 0035

The successor recovery prototype:

- scans only a caller-bounded suffix;
- caps scan requests and request size;
- charges all successful and failed candidate reads;
- caps magic matches, candidate validations, results, and linked-history depth;
- treats footer magic as a candidate hint with no authority;
- validates every reported result as an exact strict prefix;
- reports candidates without selecting an active replacement;
- rejects cycles, invalid parent links, sequence gaps, truncation, and candidate storms.

Recovery remains separate from exact-end strict validation. A caller must make any recovery-selection decision explicitly.

## Reusable Rust successor core

### Experiment 0043 and ADR-0016

A reusable Rust slice module now reproduces the exact four-object genesis, replacement append, and 400-object multi-level identities. It exposes deterministic writing, exact-end strict validation, linked-history verification, bounded suffix recovery, and strict-source full or caller-selected rewrite.

The assurance scopes are intentionally separate:

- current validity never searches for an alternative footer;
- linked history independently revalidates every prefix and fails closed rather than returning a partial chain;
- recovery treats footer magic only as a bounded hint and reports strictly validated prefixes without selecting one;
- rewrite accepts only exact-end strictly validated active state, publishes a new genesis identity, performs no semantic dependency discovery, and does not preserve byte-scoped signatures.

The focused suite covers current-page-count forgery, partial page overlap, ancestor corruption that leaves current validation successful but makes history fail, interrupted publication, recovery attempt/result caps, deterministic selected rewrite, damaged-source rejection, and allocation/output limits.

Three raw/generated/history targets plus one rewrite target extend the immutable-successor fuzz surface. The full cargo-fuzz matrix now contains twenty-five targets.

## Bounded deterministic writer and spill lifecycle

### Experiments 0013, 0016, and 0028

A bounded external sorter processes 200,003 exact 88-byte locator-shaped records using sub-megabyte runs. The sorted stream feeds canonical immutable leaf and internal-page emission directly.

| Entries/run | Runs | Peak sort bytes | Locator spill | Reference spill | Pages | Output bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 4,096 | 49 | 360,448 | 17,600,264 | 69,632 | 1,088 | 17,825,792 |
| 7,777 | 26 | 684,376 | 17,600,264 | 69,632 | 1,088 | 17,825,792 |

Both configurations and a directly sorted baseline produce identical page bytes, root digest, and output SHA-256.

A staged merger caps simultaneously open runs. Fan-in limits of 4, 8, and 32 expose the merge-pass versus descriptor trade-off while preserving exact output. Missing keys, duplicate keys including cross-run duplicates, bad record widths, and exhausted work budgets fail closed.

### Experiment 0036

A publication-lifecycle prototype adds:

- private staging directories and files;
- checked disk-byte accounting;
- descriptor-bounded staged merge;
- create-new final-path semantics;
- no-overwrite publication through a same-filesystem hard link;
- strict validation before publication;
- cleanup of abandoned staging directories;
- explicit pre-link `not published` and post-link `indeterminate but valid` outcomes.

Durability still depends on platform-specific file and directory synchronization. Confidential spill encryption, secure deletion, inode exhaustion, hostile filesystem behavior, and portable atomic publication remain unresolved.

## Locator layout

### Experiments 0010 and 0017

At 100 million objects with 16 KiB pages:

| Leaf layout | Directory size |
|---|---:|
| 88-byte Candidate 1, 64-bit ID | 8.280 GiB |
| 72-byte tight mirrored, 64-bit ID | 6.778 GiB |
| 56-byte minimal authenticated, 64-bit ID | 5.264 GiB |
| 64-byte minimal authenticated, 128-bit ID | 6.007 GiB |

The 56-byte locator transfers fewer total bytes than a tight 72-byte mirrored locator only below approximately 33.9% metadata-inventory coverage. Above that threshold, object-header reads outweigh the directory saving and may add one range request per inspected object.

Current conclusions:

- retire Candidate 1's 16-byte per-leaf reserve unless a concrete successor use is justified;
- decide identifier width separately from metadata mirroring;
- measure sparse lookup and broad inventory as distinct workloads;
- consider an optional inventory structure rather than forcing one primary locator to optimize both.

## Source and transport semantics

### ADR-0013, ADR-0014, and Experiment 0018

One assurance operation uses one strong expected source-version token. Conditional retries are permitted only against that same token. Partial responses are discarded and charged. Version mismatch, cancellation, deadline, or retry exhaustion terminates the operation.

A new token requires a clean restart with fresh parser, digest, traversal, diagnostic, and output state. Stable view prevents mixed-version reads; it does not prove newest-version freshness.

## Resource limits

### ADR-0015 and Experiment 0019

Candidate 1 defaults are independent implementation safety ceilings, not one jointly satisfiable conformance profile. For example:

- `max_objects = 10,000,000`;
- `max_read_operations = 1,000,000`.

Even an unrealistically optimistic one-read-per-object validator requires ten times the configured operation budget.

A future support profile must choose file, object, page, read, hash, allocation, recovery, spill, and output minima jointly and publish boundary tests. Resource-policy refusal remains distinct from malformed-file rejection.

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

Unknown dependency semantics must abort or invoke an explicit conservative retain-all-unknown policy. Cycles use visited tracking; missing dependencies and snapshot, node, edge, and depth limits fail closed.

`repair-all`, caller-selected rewrite, and semantic compaction remain three distinct assurance claims.

## External freshness

### Experiment 0022

Internal hashes, sequence, parent links, and verified history cannot detect replacement with an older complete valid whole file.

- TOFU protects only after a trusted observation.
- Trusted state should include exact identity as well as ordering to detect same-sequence forks.
- Trusted-state updates need atomic application semantics.
- Multi-device state requires secure synchronization.
- Online transparency can detect rollback and forks but introduces availability, privacy, witness, and proof-policy dependencies.

Phase 3 exposes snapshot identity, commit identity, sequence, roots, and verified history. It makes no freshness claim.

## Hostile-byte and independent-vector evidence

### Experiment 0029

Twelve successor page mutations recompute outer authentication where necessary and reach intended checks for magic, kind, level, entry width, ordering, child ranges, padding, page digests, and physical ranges.

### Experiment 0037

The pinned `genesis-four` vector is:

- generated and strictly validated by the Python successor model;
- exactly 16,886 decoded bytes;
- pinned to SHA-256 `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23`;
- exact-end with one footer and no trailing bytes;
- independently parsed and hashed from raw fields by Rust;
- checked for object, page, snapshot, and commit digest agreement, ordering, canonical padding, and physical overlap.

The experiment discovered and replaced an earlier malformed checked-in fixture that contained no footer magic. This is one genesis vector, not an independent complete implementation or invalid corpus.

## Evidence status

Green read-only workflows now exist for:

- page identity alternatives and Candidate 1 page-reuse rejection;
- immutable-page COW and batching;
- leaf and recursive internal splits;
- recursive deletion and underflow handling;
- deterministic operation sequences and multi-seed property campaigns;
- complete object records, insertion, deletion, and verified history;
- bounded source lookup and strict validation;
- authenticated roots, capabilities, and extension preservation;
- bounded recovery without candidate selection;
- reusable Rust successor writing, strict validation, linked history, recovery, rewrite, and fuzz targets;
- spill-backed page emission, staged merge, and publication lifecycle;
- locator inventory crossover;
- stable-source retry and restart semantics;
- limit interactions;
- semantic retention inputs;
- external freshness models;
- layer-targeted adversarial bytes;
- the manifest-pinned, independently parsed successor vector.

These workflows supplement rather than replace Candidate 1 Rust, Python, invalid-vector, portability, and fuzz evidence.

## Remaining successor blockers

Before an immutable-page successor can be proposed as an independently implementable epoch, it still needs:

1. one complete byte specification covering file, object, immutable page, catalog/extension, snapshot, and footer structures;
2. one selected identifier width, locator layout, occupancy rule, split policy, and deletion policy;
3. a general deterministic mixed-operation batch planner across replacements, insertions, and deletions at arbitrary depth;
4. production random-access/conditional source, streaming or spill-integrated writer, and hardened repair/publication paths beyond the reusable Rust slice core;
5. cross-language multi-level, append, recovery, fork, and compaction vectors;
6. a pinned successor invalid and interrupted corpus with coarse diagnostic classes;
7. jointly satisfiable support profiles and boundary vectors;
8. production spill confidentiality, cleanup, durability, and portable publication policy;
9. conditional HTTP/cloud adapters and asynchronous cancellation tests under stable-view rules;
10. arbitrary-depth operation and hostile-source fuzzing for the selected implementation;
11. an independently maintained implementation or external independent review;
12. external freshness policy where applications require rollback resistance;
13. maintainer review and explicit retirement, retention, or supersession of Candidate 1.

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
- `docs/experiments/0025-exp0002-immutable-page-internal-split.md`
- `docs/experiments/0026-exp0002-content-reversion.md`
- `docs/experiments/0027-exp0002-immutable-page-recursive-delete.md`
- `docs/experiments/0028-exp0002-staged-spill-merge.md`
- `docs/experiments/0029-exp0002-immutable-page-adversarial.md`
- `docs/experiments/0030-exp0002-immutable-page-property-campaign.md`
- `docs/experiments/0031-exp0002-immutable-page-objects.md`
- `docs/experiments/0032-exp0002-immutable-page-object-history.md`
- `docs/experiments/0033-exp0002-immutable-page-object-source.md`
- `docs/experiments/0034-exp0002-immutable-page-metadata.md`
- `docs/experiments/0035-exp0002-immutable-page-recovery.md`
- `docs/experiments/0036-exp0002-spill-publication.md`
- `docs/experiments/0037-exp0002-immutable-successor-vector.md`
- `docs/experiments/0043-immutable-successor-rust-api.md`
- `docs/decisions/0013-exp0002-versioned-source-stability.md`
- `docs/decisions/0014-exp0002-restart-whole-operation-on-version-change.md`
- `docs/decisions/0015-exp0002-resource-defaults-are-policy.md`
- `docs/decisions/0016-immutable-successor-rust-assurance-scopes.md`
