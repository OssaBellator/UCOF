# Phase 3 Status — Directory, Snapshots, and Recovery

**Status:** In progress; EXP-0002 Candidate 1 is executable and rejected as a reusable-page design, while an immutable-page successor is under executable evaluation  
**Started:** 2026-07-30  
**Working branch:** `phase-3/directory-snapshots-recovery`  
**Stacked pull request:** #3  
**Depends on:** Phase 2 pull request #2

## Objective

Deliver bounded random access, append publication, snapshots, previous-root recovery, repair, and compaction while preserving the rule that damaged, recovered, historical, partially interpreted, or merely plausible state never silently becomes active valid state.

## Current epoch boundaries

### EXP-0002 Candidate 1

Candidate 1 is a complete disposable experiment with exact bytes documented in `docs/spec/EXP_0002_BYTE_CANDIDATE.md`.

It provides:

- a 64-byte bootstrap header;
- 48-byte object records;
- 16 KiB authenticated directory pages;
- 88-byte leaf and 64-byte internal entries;
- complete snapshot records and 160-byte exact-end footers;
- domain-separated SHA-256 identities;
- deterministic Rust and Python writers;
- strict slice and bounded source validation;
- targeted authenticated lookup and absence;
- explicit recovery and verified linked history;
- repair and caller-selected rewrite;
- a separate experimental CLI.

Candidate 1 remains unpublished and has no compatibility promise.

### Immutable-page successor microformat

The successor experiments remove active snapshot sequence from page identity and use immutable content-addressed pages. They are executable evidence, not a proposed Candidate 2 epoch.

They currently cover complete objects, immutable pages, recursive tree updates, bounded source access, metadata catalogs, recovery, spill-backed writing, and one independently parsed exact-end vector. No successor compatibility promise exists.

## Accepted experimental decisions

- Phase 3 uses disposable epoch `UCOF-EXP-0002` because directory and active-root semantics change validity.
- Strict validation is exact-end and never invokes recovery implicitly.
- Recovery is explicitly requested, independently bounded, and never selects a candidate.
- Structural snapshot identity and file-instance commit identity are separate scopes under ADR-0011.
- Candidate 1 checkpoints are ordinary complete commits under ADR-0012.
- Stable source access requires strong caller-supplied version evidence under ADR-0013.
- Version change, cancellation, deadline, or exhausted retries terminate one assurance operation; a new version starts clean under ADR-0014.
- Current implementation limits are policy ceilings, not normative conformance minima, under ADR-0015.
- Repair and rewrite accept only fully verified sources and publish new commit identity.
- Candidate 1 page-sequence semantics are rejected for any successor promising historical page reuse.
- Immutable content identity is the current successor research direction; it does not establish authenticity or freshness.

## Candidate 1 implementation status

### Codec and source access

| Frontier | Status |
|---|---|
| Bootstrap, objects, pages, snapshots, and footer | Implemented and tested |
| Domain-separated object/page/snapshot/commit digests | Implemented and tested |
| Deterministic genesis and append writers | Implemented in Rust and Python |
| Strict slice validator | Implemented |
| Bounded strict source validator | Implemented |
| Targeted authenticated lookup and absence | Implemented |
| Stable-view source adapter | Implemented |
| Exact-end strict/recovery separation | Implemented |
| Physical overlap and canonical padding checks | Implemented |

Targeted lookup authenticates the active commit, snapshot, one directory path, and the selected object or absence result. It does not claim unrelated historical objects were rehashed.

A localhost HTTP Range benchmark over an append file containing an unrelated 1 MiB historical object measured:

| Assurance mode | Requests | Bytes transferred | Objects hashed |
|---|---:|---:|---:|
| Targeted lookup | 7 | 16,993 | 1 |
| Full strict validation | 26 | 1,065,673 | 3 |

Targeted lookup did not request the large historical payload; full validation did.

### Publication, recovery, and history

| Frontier | Status |
|---|---|
| Footer-only publication | Implemented |
| Incomplete latest footer rejection | Implemented and tested at every cut |
| Bounded backward discovery | Implemented |
| Failed candidate work accounting | Implemented |
| Candidate magic without authority | Implemented |
| Linked-history validation | Implemented |
| Root and identity reporting per verified prefix | Implemented |
| Candidate-storm and chain-depth limits | Implemented |
| Complete checkpoints | Implemented as ordinary commits |

Every reported recovery result is a strict valid prefix. Neither strict validation nor recovery silently chooses an alternative active state.

### Repair, rewrite, and CLI

| Frontier | Status |
|---|---|
| Verified-source repair to new file | Implemented |
| Caller-selected object rewrite | Implemented |
| Object, payload, and output-byte limits | Implemented |
| Damaged-source rejection | Implemented |
| Snapshot/commit identity reporting | Implemented |
| Automatic semantic dependency discovery | Not implemented; requires profile/application input |

The `ucof-exp0002` binary exposes distinct `verify`, `roots`, `history`, `lookup`, `recover`, `repair-all`, and `rewrite-selected` commands. `verify` never invokes recovery, and `rewrite-selected` does not claim semantic compaction.

### Candidate 1 corpora and continuous testing

The valid corpus contains deterministic genesis, append, and multi-leaf files. Rust and Python reproduce the same bytes.

The invalid corpus contains thirteen deterministic files covering bootstrap, object, page, layout, snapshot, parent-chain, exact-end, and interrupted-publication failures. Python regenerates them and Rust independently rejects them.

The branch also carries layer-targeted adversarial mutations, Rust 1.85 checks, 32-bit and big-endian compilation, and twenty-one cargo-fuzz targets.

## Candidate 1 architectural rejection

Candidate 1 stores and authenticates the active snapshot sequence in every directory page. Validation requires page sequence equality with the active snapshot.

An unchanged historical page therefore cannot be reused. Re-encoding the page changes its digest and propagates through every ancestor. At 100 million objects, the Candidate 1 model rewrites approximately 8.89 GB of directory pages for one changed object, while an immutable no-split path rewrites 64 KiB.

This is a byte-design blocker, not merely an unfinished writer optimization. Candidate 1 may still serve as disposable evidence, but it is not a suitable reusable-page successor baseline.

## Immutable-page successor evidence

Detailed evidence is consolidated in `docs/PHASE_3_SUCCESSOR_EVIDENCE.md`.

### Page identity and persistent updates

The successor microformat demonstrates:

- exact reuse and deduplication of unchanged pages;
- strict traversal of mixed-age pages;
- deterministic single-path replacement;
- two-leaf batched path sharing independent of input order;
- insertion routing across sparse child ranges;
- leaf split, sibling merge, and redistribution;
- root-height increase and collapse;
- recursive internal split and recursive deletion/underflow;
- exact reuse of an earlier root when an inverse update restores identical contents.

A reused historical root does not reuse publication identity: the new commit still receives new sequence, parent, snapshot, and commit identities.

### Operation campaigns

- one deterministic 512-operation sorted-set differential sequence;
- 34 deterministic seeds;
- 256 operations per seed;
- 8,704 operations total;
- deterministic replay and exact oracle agreement after every operation;
- bounded page-emission and root-transition checks.

The campaign still concentrates on a constrained height-one random envelope. Recursive depth boundaries are tested separately rather than fuzzed together.

### Complete objects and historical assurance

Successor complete-object experiments implement:

- real 48-byte object records and payloads;
- object and locator cross-checks;
- domain-separated object digests;
- object/object and object/structural overlap rejection;
- deterministic replacement, insertion, and deletion;
- historical object reuse;
- active-snapshot versus verified-history assurance separation.

Corrupting a deleted historical object can leave the active snapshot valid while verified history rejects the ancestor that references it. This distinction is intentional.

### Bounded successor source access

A random-access source prototype implements:

- targeted authenticated lookup and absence;
- full exact-end validation;
- bounded request size, operations, bytes, pages, objects, hash work, and allocation;
- chunked commit and object hashing;
- proof that targeted lookup skips an unrelated 1 MiB payload that full validation reads.

The prototype uses a stable in-memory source. Concrete conditional HTTP/cloud adapters and asynchronous cancellation remain pending.

### Roots, capabilities, and extension preservation

An authenticated catalog object carries sorted roots, sorted capabilities, required criticality, and canonical extension records.

- unknown required capabilities preserve structural-integrity evidence but block interpretation;
- unknown optional extensions survive catalog replacement byte-for-byte;
- missing roots, duplicate/zero roots, malformed capability ordering, unknown flags, malformed extensions, and work-limit violations fail closed.

No normative capability allocation has been selected.

### Successor recovery

The successor recovery model:

- bounds suffix bytes, requests, request size, matches, validations, results, chain depth, and cumulative reads;
- charges failed candidate work;
- validates every result as a strict prefix;
- reports but never selects candidates;
- rejects cycles, sequence gaps, invalid parents, truncation, and candidate storms.

### Bounded writer and publication lifecycle

The writer experiments provide:

- bounded external sorting of 200,003 locator-shaped records;
- canonical immutable page emission directly from the sorted stream;
- fixed-width page-reference spill levels;
- staged descriptor-limited merging at fan-in 4, 8, and 32;
- output identical to a directly sorted baseline;
- private staging, disk budgets, create-new final-path semantics, no-overwrite publication, and abandoned-stage cleanup;
- explicit pre-publication and post-link-indeterminate outcomes.

Production spill confidentiality, secure deletion, inode exhaustion, platform durability, and portable atomic publication remain unresolved.

### Independently parsed successor vector

The non-normative `genesis-four` vector is pinned by manifest:

- decoded length: 16,886 bytes;
- SHA-256: `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23`;
- exact-end footer with no trailing bytes;
- object identifiers 1–4 and payloads `alpha`, `bravo`, `charlie`, and `delta`.

Python generates and strictly validates the file. A separate Rust integration test parses raw fixed fields and independently verifies object, page, snapshot, and commit hashes, ordering, padding, locator claims, and physical overlap without using the Python validator or Candidate 1 parser.

The independent workflow discovered and replaced an earlier malformed checked-in fixture containing no footer magic.

## Measured successor trade-offs

At 100 million objects and 16 KiB pages:

| Leaf layout | Approximate directory size |
|---|---:|
| 88-byte Candidate 1, 64-bit ID | 8.280 GiB |
| 72-byte tight mirrored, 64-bit ID | 6.778 GiB |
| 56-byte minimal authenticated, 64-bit ID | 5.264 GiB |
| 64-byte minimal authenticated, 128-bit ID | 6.007 GiB |

A 56-byte locator transfers fewer bytes than a 72-byte mirrored locator only below approximately 33.9% metadata-inventory coverage. Identifier width and metadata mirroring remain separate decisions.

Current resource defaults are not a coherent conformance class. For example, a ten-million-object ceiling conflicts with a one-million-read ceiling even at an optimistic one read per object.

## Security boundaries

- SHA-256 integrity is not authenticity, confidentiality, provenance, signer trust, or freshness.
- Stable source version prevents mixed-version reads but does not prove the latest version.
- Whole-file rollback remains undetectable without trusted external state.
- Unknown required capabilities block interpretation but do not erase integrity evidence.
- Unknown optional fields require explicit preservation policy during repair, rewrite, and compaction.
- Semantic compaction requires a history-retention policy and profile/application dependency semantics.
- Byte-scoped signatures are not preserved by rewrite.
- Both current implementations live in one repository and may share a specification misunderstanding.

## Continuous verification

Permanent workflows use read-only repository permissions.

The current matrix covers:

- locked dependencies;
- rustfmt and clippy with warnings denied;
- workspace, documentation, integration, and CLI tests;
- Candidate 1 Rust/Python valid and invalid corpora;
- independent Phase 3 model cases;
- immutable page identity, update, split, delete, history, source, metadata, recovery, and publication experiments;
- hostile-byte cases;
- the manifest-pinned independently parsed successor vector;
- Rust 1.85, 32-bit, and big-endian compilation;
- twenty-one cargo-fuzz target builds and bounded smoke campaigns.

## Current limitations and blockers

### Candidate 1

- page-sequence semantics prohibit historical page reuse;
- the append writer rebuilds all directory pages;
- locator width and identifier width remain unresolved;
- readers are synchronous;
- rewrite commands materialize source and output in memory;
- no authenticity, confidentiality, signatures, provenance, or external freshness exists.

### Successor microformat

- no complete Candidate 2 byte specification exists;
- identifier, locator, occupancy, split, and deletion policies are not selected normatively;
- no general arbitrary-depth mixed-operation batch planner exists;
- successor implementations remain Python experiments plus one independent Rust vector parser;
- only one pinned successor genesis vector exists;
- no pinned successor invalid/interrupted/fork corpus exists;
- no cross-language successor append, multi-level, recovery, or compaction corpus exists;
- no production successor writer, source adapter, recovery, history, or repair library exists;
- support profiles and boundary vectors are unresolved;
- production spill confidentiality and durability policy is unresolved;
- arbitrary-depth operation and hostile-source fuzzing is unresolved;
- independent external review is absent.

## Next frontier tasks

1. Build a pinned successor invalid and interrupted corpus with coarse validation layers.
2. Pin successor append and multi-level vectors and parse them independently in Rust.
3. Write a complete provisional successor byte specification without allocating a stable epoch.
4. Implement a general deterministic mixed-operation batch planner at arbitrary depth.
5. Move successor parsing and validation into a reusable Rust experiment module, then add fuzz targets.
6. Add conditional remote-source and asynchronous cancellation tests under stable-view rules.
7. Define jointly satisfiable support profiles and boundary vectors.
8. Select identifier width, locator layout, occupancy, split, and deletion policy.
9. Define production spill confidentiality, cleanup, and durability requirements.
10. Obtain independently maintained implementation or external review.
11. Resolve FCP-0002 objections and record maintainer disposition of Candidate 1.

## Exit rule

Phase 3 is not complete until a selected experimental layout demonstrates bounded source lookup and validation, append publication, safe page reuse or an explicitly accepted alternative, previous-root recovery, linked history, unambiguous active-root rules, repair, semantic-compaction inputs, cross-language valid and invalid vectors, hostile-input evidence, continuous fuzzing, realistic range-I/O measurements, deterministic large-writer strategy, documented rejected alternatives, independent review, and maintainer disposition of FCP-0002 blockers.
