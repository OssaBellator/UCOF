# Phase 3 Successor Evidence — Immutable Pages and Bounded Writers

**Status:** Executable non-normative evidence; not Candidate 2 and not an independently implementable epoch  
**Date:** 2026-07-30  
**Related proposal:** FCP-0002  
**Predecessor:** `UCOF-EXP-0002` Candidate 1

## Purpose

Candidate 1 is the complete disposable codec for authenticated paged directories, snapshots, bounded source access, recovery, history, repair, and caller-directed rewrite. It is also architecturally unsuitable for historical page reuse because it authenticates the active snapshot sequence inside every page.

This appendix consolidates the successor evidence produced after that rejection. The algorithms, byte microformats, vectors, Rust APIs, transport models, and writer experiments below narrow the design space but do not allocate a new epoch or create compatibility promises.

## Page identity result

Three page-identity alternatives were compared:

| Alternative | Exact reuse | Identical-content deduplication | Direct page-age binding | External freshness |
|---|---|---|---|---|
| Active snapshot sequence | No | No | Yes | No |
| Page birth sequence | Yes | No | Yes | No |
| Immutable content identity | Yes | Yes | No | No |

At 100 million objects, Candidate 1 rewrites 542,671 pages or 8,891,121,664 bytes for one changed object. A no-split immutable tree rewrites four pages or 65,536 bytes.

**Current successor direction:** immutable content identity. Membership is authenticated through child, root, snapshot, and commit digests. Page identity establishes neither age nor freshness.

## Byte-level persistent tree evidence

The immutable-page microformat demonstrates:

- exact reuse of unchanged historical pages;
- strict traversal of mixed-age pages;
- deterministic single-path replacement;
- input-order-independent two-leaf batching with shared ancestors;
- insertion routing across sparse child ranges;
- deterministic leaf split, sibling merge, and redistribution;
- root-height increase and collapse;
- recursive internal split propagation;
- recursive underflow propagation and deletion;
- exact reuse of unaffected sibling and cousin subtrees;
- exact resurrection of an earlier historical root after an inverse update;
- duplicate insertion and missing deletion rejection.

A new publication that reuses an old root still receives a new sequence, parent snapshot identity, snapshot digest, and commit digest.

## Operation campaigns and mixed batching

Differential evidence includes:

- one fixed 512-operation sorted-set sequence;
- 34 deterministic seeds;
- 256 operations per seed;
- 8,704 operations total;
- deterministic replay and exact oracle agreement after every operation;
- page ordering, range, digest, root-boundary, and emission-limit checks.

A separate arbitrary-depth modeled batch canonicalizes replacements, insertions, and deletions together, reuses exact historical pages, emits only final reachable pages, and produces byte-identical output for shuffled caller order.

The mixed planner is executable evidence but is not yet integrated into the reusable Rust byte writer.

## Complete objects and assurance scopes

The successor object microformat includes real 48-byte object headers and opaque payloads. Strict validation:

- cross-checks identifier, kind, record length, payload length, logical length, and locator claims;
- recomputes domain-separated object digests;
- rejects object/object and object/structural overlap;
- detects mutation of reused historical objects;
- supports deterministic replacement, insertion, and deletion;
- preserves historical object bytes while active reachability changes.

Active validation authenticates only the active snapshot. Verified history validates every linked ancestor as a separate strict prefix. Corrupting an object deleted from the active state can therefore leave the active commit valid while linked-history validation correctly rejects its ancestor.

No API silently upgrades historical or recovered state into active validity.

## Reusable Rust experiment module — Experiment 0046

`ucof-experiments::immutable_successor` exposes reusable synchronous Rust APIs for:

- deterministic genesis construction;
- deterministic replacement append;
- exact-end strict slice validation;
- verified linked-history traversal;
- bounded suffix recovery without candidate selection;
- verified-source rewrite of all active objects;
- verified-source caller-selected rewrite;
- bounded random-access full validation;
- bounded authenticated lookup and absence.

The module has explicit limits for file bytes, objects, pages, depth, allocation, output, history entries, recovery scan bytes, recovery attempts, and recovery results.

### Cross-language identities

The reusable Rust writer reproduces the independently generated Python identities:

| Recipe | Bytes | SHA-256 |
|---|---:|---|
| Four-object genesis | 16,886 | `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23` |
| Replacement append | 33,550 | `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc` |
| 400-object multi-level genesis | 89,316 | `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70` |

Input order is canonicalized before publication. A reauthenticated false current-page count and partially overlapping pages are rejected.

### Exact-end validity

`validate` accepts only the footer at the physical end and never invokes recovery.

### Verified history

`validate_history`:

- revalidates every linked prefix;
- enforces strictly decreasing physical footer offsets;
- enforces exact sequence decrements;
- cross-checks parent snapshot identity;
- enforces a caller history-depth limit;
- rejects ancestor corruption even when the newest commit remains valid.

### Report-only recovery

`scan_recovery_candidates`:

- scans only a caller-bounded suffix;
- caps footer attempts and returned candidates independently;
- treats footer magic as a hint with no authority;
- reports only exact strictly validated prefixes;
- orders results by physical recency;
- never selects an active replacement.

### Rewrite

`rewrite_all` and `rewrite_selected` require a strictly valid active source and publish a new genesis file. Caller-selected rewrite performs no semantic dependency discovery. Byte-scoped signatures are explicitly reported as not preserved.

### Random-access source API

The source API separates:

- full exact-end active-state validation;
- targeted authenticated lookup or absence.

Targeted lookup authenticates the current commit, snapshot, one root-to-leaf path, and the selected object. It does not claim that unrelated objects were rehashed.

The source trait has an explicit emptiness convenience contract. The implementation remains synchronous.

## Source and transport evidence

A bounded source prototype limits request size, operation count, bytes read, pages, objects, hash work, and allocation. Commit and object digests are streamed in bounded chunks.

A fixture containing an unrelated 1 MiB historical object demonstrates that targeted lookup skips that payload while full validation reads it.

One remote assurance operation uses one strong expected version token. Same-token retries discard and charge partial responses. Version mismatch, cancellation, deadline, or retry exhaustion terminates the operation. A new token requires a clean restart with fresh parser, digest, traversal, diagnostic, and output state.

Stable view prevents mixed-version reads. It does not prove newest-version freshness.

Source-based linked-history and recovery traversal without whole-file materialization remain pending; the reusable linked-history and recovery APIs currently operate on slices.

## Roots, capabilities, and extension preservation

An authenticated catalog object carries:

- sorted root identifiers;
- sorted capability declarations;
- required criticality;
- canonical extension records;
- byte-preserved unknown optional extension data.

Unknown required capabilities preserve structural and cryptographic evidence but block interpretation. Missing roots, duplicate or zero roots, malformed ordering, unsupported flags, malformed extensions, and work-limit violations fail closed.

No normative capability allocation has been selected.

## Recovery and fork recipes

Deterministic recipes demonstrate:

- two valid equal-sequence children of one genesis;
- independent validation of both fork terminals;
- enumeration without selection;
- linked-history validation from each terminal to genesis;
- interrupted latest publication exposing only older complete prefixes;
- reauthenticated parent-digest and sequence-gap rejection.

Internal sequence and parent links authenticate relationships but do not establish external freshness.

## Bounded deterministic writer and spill lifecycle

A bounded sorter processes 200,003 fixed-width locator-shaped records using sub-megabyte runs. The sorted stream feeds canonical leaf and internal-page emission directly.

| Entries/run | Runs | Peak sort bytes | Locator spill | Reference spill | Pages | Output bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 4,096 | 49 | 360,448 | 17,600,264 | 69,632 | 1,088 | 17,825,792 |
| 7,777 | 26 | 684,376 | 17,600,264 | 69,632 | 1,088 | 17,825,792 |

Both configurations and a directly sorted baseline produce identical page bytes and root identity.

A staged merger caps simultaneously open runs at fan-in 4, 8, and 32 while preserving exact output. Missing keys, duplicate keys, bad widths, and exhausted work budgets fail closed.

The spill-policy model adds:

- private mode-0700 workspaces and mode-0600 files;
- exclusive no-follow creation;
- byte, inode, and descriptor budgets;
- caller-held ownership tokens;
- refusal to follow or remove unowned symlinks;
- strict validation before publication;
- create-new, no-overwrite publication;
- file and directory synchronization ordering;
- retirement of the staged name after successful hard-link publication;
- cleanup only of still-private single-link files;
- explicit pre-publication and post-link-indeterminate outcomes.

Production spill encryption, secure deletion, hostile filesystem behavior, platform durability guarantees, and portable atomic publication remain unresolved.

## Locator and support-profile evidence

At 100 million objects with 16 KiB pages:

| Leaf layout | Approximate directory size |
|---|---:|
| 88-byte Candidate 1, 64-bit ID | 8.280 GiB |
| 72-byte mirrored, 64-bit ID | 6.778 GiB |
| 56-byte minimal authenticated, 64-bit ID | 5.264 GiB |
| 64-byte minimal authenticated, 128-bit ID | 6.007 GiB |

A 56-byte locator transfers fewer bytes than a 72-byte mirrored locator only below approximately 33.9% metadata-inventory coverage. Identifier width and metadata mirroring remain separate decisions.

Current implementation defaults are not a coherent support class. Jointly satisfiable small, medium, and large research profiles now demonstrate how file, object, page, read, hash, allocation, recovery, spill, and output limits must be selected together. Normative profiles remain unselected.

## Semantic compaction and freshness

Semantic compaction requires:

- a snapshot-retention policy;
- profile/application dependency semantics for every retained object kind;
- explicit fail-closed or conservative handling for unknown dependency semantics.

`repair-all`, caller-selected rewrite, and semantic compaction remain distinct claims.

Internal hashes, sequences, parent links, and verified history cannot detect replacement with an older complete valid file. Rollback resistance requires trusted external state, transparency, or another application policy. Phase 3 exposes identities and history but makes no freshness claim.

## Hostile-byte and corpus evidence

The successor evidence includes:

- a manifest-pinned exact-end genesis vector;
- append and multi-level generation recipes;
- a compact cryptographically pinned invalid/interrupted recipe corpus;
- fork and broken-parent/sequence recipes;
- layer-targeted page and object mutations that recompute outer authentication;
- independent Rust parsing and generation;
- exact coarse rejection layers without normative exception strings.

## Continuous verification

Green read-only workflows cover:

- locked Rust dependencies;
- rustfmt and Clippy with warnings denied;
- workspace, documentation, integration, and CLI tests;
- Rust 1.85, i686, and powerpc64 compilation;
- Candidate 1 valid and invalid corpora;
- immutable page algorithms, source access, metadata, recovery, and spill publication;
- cross-language successor vectors and invalid recipes;
- 24 cargo-fuzz targets, including successor strict validation, writer roundtrip, and linked history/recovery.

## Remaining successor blockers

Before an immutable-page successor can be proposed as an independently implementable epoch, it still needs:

1. an explicit Candidate 2 or other successor epoch proposal based on the non-epoch byte draft;
2. normative identifier, locator, occupancy, split, redistribution, and deletion policies;
3. arbitrary-depth mixed batching integrated into the reusable Rust byte writer;
4. source-based linked-history and recovery without whole-file materialization;
5. production repair and semantic-compaction implementation with dependency and preservation policy;
6. compaction and selected-profile boundary vectors;
7. concrete HTTP/cloud conditional adapters and asynchronous cancellation testing;
8. production spill confidentiality, durability, cleanup, and portable publication requirements;
9. broader hostile-source and selected-implementation arbitrary-depth fuzzing;
10. independently maintained implementation or external independent review;
11. application freshness policy where rollback resistance is required;
12. maintainer disposition of Candidate 1 and FCP-0002 objections.

## Key references

- `docs/spec/EXP_0002_IMMUTABLE_SUCCESSOR_DRAFT.md`
- `docs/experiments/0046-exp0002-reusable-rust-successor-api.md`
- `docs/decisions/0016-exp0002-successor-assurance-boundaries.md`
- `docs/PHASE_3_STATUS.md`
