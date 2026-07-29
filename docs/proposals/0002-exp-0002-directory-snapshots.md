# FCP-0002: EXP-0002 paged directory, snapshots, and recovery

- **Status:** Draft
- **Authors:** UCOF maintainers
- **Created:** 2026-07-30
- **Last updated:** 2026-07-30
- **Target:** Core
- **Experimental epoch impact:** New epoch required
- **Related issues:** None yet
- **Related ADRs:** ADR-0009
- **Supersedes:** None
- **Superseded by:** None

## Summary

This proposal defines the research scope for `UCOF-EXP-0002`, a new disposable epoch that adds the access and durability properties intentionally absent from EXP-0001:

- a mandatory paged primary directory whose single-object lookup does not require decoding every entry;
- append-only snapshot publication with explicit parent relationships and monotonic sequence rules;
- exact-end strict validation separated from bounded recovery candidate discovery;
- independently readable checkpoints for long-running or interrupted production;
- valid-root enumeration, orphan reporting, repair-to-new-file, and reachability-based compaction.

FCP-0002 does not yet fix final wire field widths or page sizes. It establishes invariants, candidate layouts, required experiments, security boundaries, and exit evidence. The proposal must not enter Review until an independent implementation can reproduce the selected layout and all unresolved byte choices are removed.

## Motivation

EXP-0001 demonstrates framing, deterministic metadata, exact-end publication, integrity checking, and a checked directory accelerator. It also produces a decisive negative result: one million zero-byte objects require approximately 40 MB of record headers and 52 MB of flat directory metadata before payloads. A reader must materialize every directory entry before one lookup. Raising limits cannot repair that architecture.

EXP-0001 also has one active root and no append history. An interrupted append produces trailing bytes after the previous footer. Strict exact-end validation correctly rejects the damaged tail, but the format has no normative way to enumerate the previous valid root, distinguish a stale root from a newer valid root, or compact a selected history.

This proposal addresses Phase 0 use cases involving massive object counts, append-only capture, interrupted writes, bounded remote range access, historical snapshots, and repair without destructive mutation. It directly addresses threat-model items covering malicious directories, stale-root selection, rollback ambiguity, recovery candidate exhaustion, range confusion, and false validity.

## Scope

FCP-0002 defines or requires definition of:

1. the EXP-0002 bootstrap and commit-discovery relationship;
2. paged primary-directory invariants;
3. leaf and internal page semantics;
4. snapshot identity, sequence, parent, and active-root rules;
5. strict exact-end validation;
6. bounded backward and sequential recovery modes;
7. complete and progress checkpoint semantics;
8. commit completeness and publication order;
9. valid-root enumeration and orphan reporting;
10. repair into a new file;
11. reachability-based compaction;
12. required limits, diagnostics, vectors, experiments, and cross-language evidence.

## Non-goals

This proposal does not define:

- compression or transform pipelines;
- encryption, signatures, signer trust, or provenance claims;
- schemas or profile semantics;
- remote-reference retrieval;
- concurrent multi-writer conflict resolution;
- distributed consensus or globally monotonic freshness;
- automatic in-place repair;
- stable UCOF Core 1.0 bytes;
- a promise to migrate EXP-0001 or EXP-0002 files.

## Terminology

**Commit** — a complete append publication consisting of new records, a directory root, a snapshot manifest, and a commit footer.

**Snapshot** — the logical object graph selected by one complete commit.

**Snapshot sequence** — a file-local unsigned integer that must increase along the selected parent chain. It is ordering evidence, not freshness proof against rollback.

**Parent snapshot** — the immediately preceding snapshot identity declared by a snapshot, or absent for genesis.

**Directory page** — a bounded node in the primary directory. A page is either internal or leaf.

**Directory root** — the page locator and authenticated identity from which all primary-directory lookup paths begin.

**Strict mode** — validation that accepts only a complete footer at the exact physical end of the source.

**Recovery mode** — explicitly requested bounded discovery of earlier complete commit candidates. Recovery success is not equivalent to proof that no newer snapshot existed.

**Complete checkpoint** — a fully independently readable snapshot published before a larger logical operation is finished.

**Progress checkpoint** — a non-root progress marker that cannot be selected as an active snapshot.

**Orphan record** — a physically complete record not reachable from a selected valid snapshot under the applicable graph rules.

## Detailed specification

### 1. Experimental epoch

The selected layout will use `UCOF-EXP-0002`. EXP-0001 readers must report the epoch as unsupported. EXP-0002 readers must not guess EXP-0001 semantics from similar magic or field positions.

The new epoch is required because snapshot and directory discovery change validity, not merely optional metadata.

### 2. File and commit organization

A file contains:

1. one fixed bootstrap header;
2. zero or more complete commits;
3. optionally, an incomplete trailing append.

A complete commit is written in this logical order:

1. new or replacement object records;
2. primary-directory leaf pages;
3. primary-directory internal pages, with the root written last among directory pages;
4. a snapshot manifest record;
5. a fixed-size commit footer written last.

Publication occurs only when the complete footer write succeeds. No earlier record or checkpoint may be interpreted as publishing that commit unless it is itself a separately complete snapshot.

An interrupted append must not invalidate the bytes or identity of an earlier complete commit. Strict validation may reject the full damaged source because the exact end is not a footer; recovery mode may enumerate earlier candidates under explicit budgets.

### 3. Snapshot manifest requirements

A snapshot manifest must contain at least:

- snapshot sequence;
- optional parent snapshot identity;
- root object identifiers;
- primary-directory root locator and identity;
- required and optional capabilities;
- commit-scoped object or record count;
- history-retention declaration where applicable;
- checkpoint classification;
- canonical snapshot identity inputs.

A complete snapshot must not reference a progress checkpoint as its parent.

Root object identifiers must resolve through the selected primary directory. The directory root and snapshot manifest must be mutually bound by the commit identity so an unauthenticated pointer cannot redirect one to unrelated pages.

### 4. Snapshot sequence and parent chain

For a non-genesis snapshot:

- the parent identity must resolve to a complete valid snapshot when that history is retained;
- the child sequence must be strictly greater than the parent sequence;
- the initial experiment should require `child.sequence = parent.sequence + 1` unless evidence supports gaps;
- cycles are invalid;
- two distinct valid candidates with the same identity inputs but different bytes are invalid;
- two unrelated highest-sequence candidates form an ambiguous fork and must not be selected silently.

Sequence is file-local ordering metadata. It does not establish freshness against replacement of the entire file with an older valid copy.

### 5. Primary directory

The primary directory is authoritative for locating records within one snapshot but must be authenticated by the snapshot or commit identity. It is not trusted before that binding is validated.

The directory is a canonical ordered search tree with bounded pages.

Each leaf entry must describe at least:

- object identifier;
- object kind or type reference;
- physical record offset;
- stored length;
- logical length when known;
- required capability summary or reference;
- transform-pipeline reference where applicable in later epochs;
- schema reference where applicable in later epochs;
- integrity reference or binding where applicable.

Each internal entry must describe at least:

- inclusive or exclusive key boundary with one unambiguous convention;
- child page locator;
- child page length;
- child page identity or authenticated binding;
- optional subtree entry count for diagnostics and planning, never trusted to alter semantic lookup results.

Required invariants:

- object identifiers are strictly ordered in leaves;
- child key ranges are non-overlapping and cover exactly their declared subtree ranges;
- leaf and internal node types cannot be confused;
- all page lengths are bounded before allocation;
- page depth is bounded;
- lookup rejects duplicate identifiers and contradictory ranges;
- a single-object lookup reads only the root-to-leaf path plus required authentication material;
- a full inventory can iterate leaves in canonical identifier order;
- unknown optional per-entry metadata can be skipped or preserved according to declared encoding rules;
- the directory cannot point inside its own headers or into unrelated footer bytes;
- page graph cycles are invalid.

### 6. Page sizing and encoding

The final page size, fixed versus bounded-variable sizing, entry encoding, fanout, and page identity scope remain unresolved pending experiments.

Candidate page sizes are 4 KiB, 16 KiB, and 64 KiB. The selected design must measure:

- lookup range requests;
- directory overhead at one thousand, one million, and one hundred million objects;
- page rewrite amplification between snapshots;
- small-object overhead;
- canonical encoding cost;
- remote latency sensitivity;
- corrupted-page isolation.

The proposal should prefer a page-local representation that can be validated without decoding unrelated pages. A general metadata decoder must not be required for every lookup if a smaller fixed or restricted page representation provides safer bounded access.

### 7. Commit footer

The EXP-0002 footer must be fixed-size and independently recognizable. It must include or locate, with exact scopes:

- footer version or epoch context;
- footer length and flags;
- commit start or commit byte range;
- snapshot manifest offset and length;
- directory root offset and length, directly or through the snapshot;
- previous complete footer offset when retained, or an explicit genesis value;
- snapshot sequence;
- record or page count needed for bounded validation;
- digest algorithm identifier;
- commit or snapshot digest;
- reserved bytes required to be zero.

Every footer field outside the committed digest scope must receive explicit structural and semantic validation. Algorithm identity must never be inferred from digest length.

The exact footer size and digest scope remain unresolved until truncation, append, and cross-language experiments are complete.

### 8. Strict active-root selection

Strict validation:

1. obtains the physical source length;
2. reads exactly one footer-sized range at the exact end;
3. validates footer framing and reserved fields;
4. validates all declared ranges with checked arithmetic;
5. validates the snapshot manifest and directory root binding;
6. validates the commit digest under the tagged algorithm and exact scope;
7. validates required capabilities;
8. returns the exact-end snapshot only if every required check succeeds.

Strict mode must not silently scan backward when the exact-end footer is absent or invalid.

### 9. Recovery candidate discovery

Recovery is a separate API and CLI mode. It may provide:

- bounded backward footer search;
- bounded forward commit enumeration from the bootstrap;
- caller-supplied candidate offsets;
- checkpoint-assisted discovery.

Every recovery operation must accept explicit limits for:

- bytes scanned;
- footer-magic candidates considered;
- candidate validations;
- bytes read;
- bytes hashed;
- diagnostics;
- parent depth;
- total roots returned.

A magic match is not a candidate until fixed fields and ranges pass cheap validation. A structurally plausible footer is not a valid root until its complete digest, snapshot, directory binding, and required capabilities are validated.

When multiple valid candidates are found:

1. build parent relationships by authenticated snapshot identity;
2. reject cycles;
3. reject non-increasing sequences;
4. identify maximal valid chains;
5. select a unique highest terminal snapshot only if policy permits and no equal-priority fork exists;
6. otherwise report ambiguity and return candidates without silently choosing one.

Recovery must state that a selected earlier root does not prove freshness or absence of a newer lost commit.

### 10. Checkpoints

A complete checkpoint is serialized exactly as a complete snapshot and may be selected independently. It can later become the parent of another snapshot.

A progress checkpoint:

- has a distinct required kind or capability;
- cannot be selected as an active snapshot;
- may refer to incomplete operation state;
- must state which earlier complete snapshot remains authoritative;
- must not be accepted by generic extraction or compaction as a complete root.

The initial reference implementation should support complete checkpoints first. Progress checkpoints remain experimental until a concrete long-running workload demonstrates their necessity.

### 11. Valid-root enumeration

A root enumerator returns a bounded sequence of candidates with explicit status:

- verified complete snapshot;
- structurally plausible but integrity-failed candidate;
- unsupported required capability;
- truncated candidate;
- ambiguous fork member;
- stale ancestor of another candidate;
- progress checkpoint;
- invalid candidate.

Diagnostic status must not be conflated with active-root selection.

### 12. Repair

Repair creates a new output by default. It must not mutate the damaged source in place unless a later explicitly unsafe expert mode is defined.

Repair requires an explicitly selected verified snapshot candidate. It then:

1. copies or rewrites records reachable from that snapshot;
2. rebuilds a canonical primary directory;
3. writes a new snapshot and footer;
4. records that physical bytes and commit identity changed;
5. does not claim to preserve signatures whose scope changed;
6. reports omitted or orphaned records.

Salvaged but unverified records require a separate extraction operation and cannot be silently included in a verified repaired snapshot.

### 13. Compaction

Compaction accepts:

- one selected verified snapshot;
- an optional history-retention policy;
- a destination writer;
- resource limits.

Default compaction copies only records reachable from the selected snapshot. History-preserving compaction copies the selected parent chain according to explicit policy.

Compaction must:

- traverse object and metadata dependencies with cycle detection and depth/count limits;
- rebuild directory pages;
- preserve logical object identity only where the identity scope permits physical rewriting;
- invalidate, omit, or reissue byte-scoped signatures rather than falsely preserving them;
- report orphan and unreachable records;
- produce a new file and leave the source unchanged.

### 14. Error conditions

At minimum, implementations must distinguish:

- exact-end footer absent;
- malformed footer;
- unsupported epoch or required capability;
- candidate scan limit exceeded;
- candidate count limit exceeded;
- invalid snapshot range;
- invalid directory page range;
- duplicate or unordered directory key;
- overlapping child key range;
- directory page cycle;
- missing root object;
- missing parent snapshot;
- non-increasing sequence;
- ambiguous fork;
- digest mismatch;
- progress checkpoint selected as complete;
- repair source not verified;
- compaction dependency limit exceeded;
- source I/O failure.

## Compatibility impact

### Existing files with new readers

EXP-0002 readers may support EXP-0001 through an explicit separate decoder. They must not parse EXP-0001 using EXP-0002 snapshot or directory rules.

### New files with old readers

EXP-0001 readers must reject EXP-0002 as an unsupported epoch.

### Unknown capabilities and data preservation

Unknown required snapshot, directory, checkpoint, transform, or integrity behaviour fails closed. Unknown optional metadata may be skipped or preserved only when its encoding and scope make that safe.

### Profile and schema compatibility

No profile or schema semantics are defined. Future references carried by directory entries remain opaque until their proposals are accepted.

### Canonical identity and signatures

The experiment defines digest scope but no signatures. Snapshot and directory identity must be domain-separated. Future signature proposals must not inherit ambiguous experimental scopes.

### Version or experimental epoch impact

A new epoch is mandatory: `UCOF-EXP-0002`.

### Migration and coexistence

No migration guarantee exists. A research converter may rewrite EXP-0001 objects into EXP-0002, but the result has a new commit identity and cannot claim byte-preserving continuity.

## Security impact

New attacker-controlled inputs include page boundaries, key ranges, child locators, page identities, snapshot sequence, parent identity, previous-footer pointers, candidate offsets, checkpoint kinds, and compaction graph edges.

Principal risks include:

- page cycles or fanout exhaustion;
- forged subtree ranges;
- candidate storms from repeated footer magic;
- stale-root or rollback presentation;
- fork ambiguity;
- previous-footer cycles;
- authenticated root redirected to unauthenticated pages;
- repair upgrading salvage to validity;
- compaction omitting security-relevant dependencies;
- sequence treated as external freshness proof.

The threat model must be updated with executable findings before the proposal can be accepted.

## Privacy impact

A public primary directory may expose object identifiers, kinds, counts, sizes, physical clustering, update frequency, and snapshot relationships. Parent chains reveal history depth and may reveal that objects changed even when payloads are later encrypted.

EXP-0002 will document this leakage but will not solve protected directories. Encryption and selective disclosure belong to Phase 7. Applications must not assume that append history or compaction erases previously distributed data.

## Resource-limit impact

Readers must support limits for:

- bootstrap and exact-end reads;
- directory page size;
- directory depth and page count;
- keys per page;
- lookup page reads;
- full-inventory pages and entries;
- commit bytes;
- root candidates;
- scan bytes and magic matches;
- candidate validation bytes and hashes;
- parent-chain depth;
- checkpoint count;
- orphan count;
- graph traversal nodes and edges;
- compaction output bytes;
- diagnostics.

Limits must apply before allocation, seek, range request, hash work, or recursive traversal where possible.

## Streaming impact

A non-seeking writer can append object records, directory pages, snapshot, and footer when all final locators can be computed from counted output. It may need to retain directory-building state or spill sorted runs to bounded temporary storage.

A sequential reader can consume records, but it cannot provide authenticated random lookup until the directory root and snapshot are reached. Complete checkpoints allow a long stream to publish intermediate independently readable states.

The design must not require rewriting prior payload bytes for ordinary snapshot append.

## Random-access impact

A single-object lookup should require:

1. exact-end footer read;
2. snapshot manifest read;
3. directory root read;
4. one page per directory level;
5. target record header and requested payload range.

The experiment must report range-request count and bytes read separately. Directory page authentication must not require reading all sibling pages.

## Recovery, truncation, and compaction

Strict mode remains exact-end. Recovery mode is explicit and bounded. Interrupted append leaves the earlier complete commit intact. Recovery may find it through a previous-footer chain, bounded scan, sequential enumeration, checkpoint, or caller-provided offset.

A candidate found through recovery is valid only after complete validation. A valid earlier root remains potentially stale. Ambiguous forks are reported, not guessed.

Repair and compaction write new files by default.

## Canonicalization and identity

The final proposal must define domain-separated canonical inputs for at least:

- directory page identity;
- directory root identity;
- snapshot identity;
- commit digest;
- parent link.

Physical offsets may be included in commit integrity while excluded from logical object identity. The distinction must be explicit so compaction cannot falsely preserve a byte-scoped identity.

## Alternatives considered

### Flat canonical metadata directory

Rejected for promotion because lookup requires decoding every entry and the measured metadata overhead fails the massive-object use case.

### Hash table directory

Offers expected constant lookup but complicates deterministic canonical layout, ordered iteration, adversarial collision handling, and range partitioning. It remains a benchmark alternative.

### Sorted monolithic fixed-entry array

Enables binary search and simpler implementation but large remote ranges, rewrite amplification, and authentication granularity may remain poor. It must be included as a baseline experiment.

### Front-of-file mutable superblock

Two alternating checksummed superblock slots can publish a new tail pointer atomically on many local filesystems, preserving the old pointer after interruption. This improves local recovery but requires seeking, relies on storage atomicity assumptions, and complicates streams and immutable object storage. It remains an optional deployment optimization, not the only normative discovery mechanism, unless evidence changes this decision.

### Silent backward scan in normal validation

Rejected. Attackers can fill a bounded tail with thousands of magic candidates, and fallback would blur strict validity with recovery.

### Sequence-only active-root selection

Rejected. Sequence is attacker-controlled within a valid file and cannot resolve unrelated forks, rollback of the whole source, or missing parent evidence by itself.

## Unresolved questions

Before Review, resolve:

1. fixed versus bounded-variable page size;
2. page size and maximum fanout;
3. fixed entry layout versus restricted canonical metadata;
4. B+ tree, sorted page array, or another canonical lookup structure;
5. page identity algorithm and domain separation;
6. commit footer size and exact digest scope;
7. previous-footer locator representation;
8. whether snapshot sequences permit gaps;
9. whether full parent history is mandatory, optional, or profile controlled;
10. maximum normative recovery scan and candidate limits, if any;
11. complete-checkpoint cadence guidance;
12. whether progress checkpoints remain in EXP-0002;
13. logical versus physical object identity during compaction;
14. how unknown optional directory-entry fields are preserved;
15. whether streaming writers may use bounded external sort state.

## Implementation plan

1. implement language-neutral in-memory models for paged lookup, snapshot chains, root selection, and compaction reachability;
2. benchmark B+ tree, sorted page array, and hash-page alternatives;
3. define an initial EXP-0002 byte layout in a separate experimental specification;
4. implement Rust and independent Python writers/readers;
5. publish valid, invalid, interrupted-append, fork, and compaction vectors;
6. add bounded exact-end and recovery APIs;
7. add append, checkpoint, repair, and compaction CLI experiments;
8. fuzz pages, candidate discovery, chains, and graph traversal;
9. integrate findings into the threat model;
10. revise this FCP until independent implementation is possible without reading Rust code.

## Evidence and validation

Required before acceptance:

- directory overhead at 1,000, 1,000,000, and 100,000,000 entries;
- root-to-leaf lookup reads and bytes for local and remote models;
- append amplification across small and large snapshots;
- interrupted writes at every byte boundary around snapshot and footer publication;
- old-root recovery after partial appends of increasing size;
- candidate-storm and previous-pointer-cycle attacks;
- fork and sequence ambiguity vectors;
- page overlap, duplicate key, cycle, and forged-range vectors;
- deterministic Rust/Python byte reproduction;
- fuzzing of page parsing, lookup, footer discovery, chain construction, repair, and compaction;
- 32-bit and big-endian portability checks;
- a generated massive-object test that does not materialize all entries for one lookup;
- threat-model update and public objection disposition.

## Registry allocations requested

No permanent allocations are requested while the proposal is Draft. Symbolic experimental names may be used inside EXP-0002 fixtures.

## Rollout plan

- keep all bytes under `UCOF-EXP-0002`;
- keep writers opt-in and clearly experimental;
- retain EXP-0001 test vectors as separate interoperability evidence;
- add CLI commands under explicit experimental naming;
- publish benchmark and recovery results with every material layout revision;
- do not enable automatic conversion or production storage claims.

## Rejection or rollback strategy

If the selected directory or snapshot layout fails scale, safety, recovery, or interoperability gates, retire EXP-0002 and move to a new experimental epoch. Produced files remain disposable research artifacts with no migration guarantee.

## References

- `docs/IMPLEMENTATION_PLAN.md`
- `docs/THREAT_MODEL.md`
- `docs/USE_CASES.md`
- `docs/experiments/0002-footer-discovery.md`
- `docs/experiments/0003-scale-limits.md`
- `docs/security/EXP_0001_FINDINGS.md`
- FCP-0001 and its evidence appendix

## Decision record

- **Decision:** Pending
- **Decision date:**
- **Review period:**
- **Approvers:**
- **Blocking objections and disposition:**
- **Required follow-up:**
