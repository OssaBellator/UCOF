# UCOF Phase 0 Use-Case Corpus

## 1. Purpose

This corpus turns broad claims such as “universal,” “streamable,” and “random access” into concrete workloads that can reject weak designs.

Each scenario records scale, access patterns, mutation, trust boundaries, failure modes, and acceptance questions. These are design inputs, not promises that one physical layout will optimize every scenario equally.

The corpus must be revisited before every phase exit and before UCOF Core 1.0.

## 2. Common evaluation dimensions

Every prototype should report, where relevant:

- bytes read before useful data is returned;
- peak memory and largest single allocation;
- number of seeks or range requests;
- metadata overhead;
- time to enumerate objects;
- time to locate one selected object;
- behavior after truncation at representative boundaries;
- behavior with unknown required and optional capabilities;
- whether integrity can be checked without reading unrelated payloads;
- whether unknown data can be copied without semantic decoding;
- information leaked by public metadata;
- limits required to process hostile input safely.

---

## UC-01 — Small human-created archive

### Scenario

A user packages a project folder for transfer and long-term storage. The package contains source files, documents, small images, configuration files, empty directories, and a few duplicate assets.

### Scale

- Total logical size: 5 MB to 500 MB.
- Object count: 100 to 5,000 filesystem entries.
- Typical payload size: 1 KB to 5 MB.
- Directory depth: usually below 20.

### Access pattern

- Create once, inspect occasionally, extract all or selected paths.
- Fast listing without decompressing payloads.
- Metadata and small files should not require reading the full archive.
- A basic reader that does not know the Archive profile should still enumerate and extract opaque objects where possible.

### Mutability

Mostly immutable after creation. Occasional append-only snapshot adds or replaces a small number of files before compaction.

### Trust boundaries

The archive may come from another person or an untrusted download. Filenames, links, permissions, and payloads are untrusted.

### Failure modes

- path traversal during extraction;
- symlink or hard-link escape;
- duplicate or conflicting paths;
- corrupted central metadata;
- one damaged payload making all other files inaccessible;
- malicious compression ratio;
- filename normalization collisions.

### Acceptance questions

- Can the full inventory be listed with bounded metadata reads?
- Can one file be extracted without decoding unrelated payloads?
- Can damaged objects be isolated without claiming the archive is fully valid?
- Are path and link semantics explicit enough for safe cross-platform extraction?
- Is overhead reasonable for many small files?

---

## UC-02 — Multi-gigabyte deduplicated archive

### Scenario

An organization stores periodic snapshots of build artifacts, virtual-machine layers, or research outputs containing repeated payloads across millions of logical objects.

### Scale

- Total logical size: 100 GB to 50 TB.
- Stored size: potentially much smaller through chunk-level deduplication.
- Object count: 1 million to 100 million.
- Chunk count: potentially hundreds of millions.

### Access pattern

- Point lookup by stable object or path identifier.
- Range-based remote reads from object storage.
- Bulk verification during ingestion.
- Periodic compaction and garbage collection.
- Copying unknown objects without decoding them.

### Mutability

Append-only snapshots are frequent. Old snapshots may be retained under policy and later compacted.

### Trust boundaries

Writers may be trusted while stored media, transport, indexes, and restoration inputs are not. Multiple teams may produce objects using different software versions.

### Failure modes

- directory too large to load in memory;
- adversarial hash collisions or algorithm confusion;
- reference cycles or excessively deep dependency chains;
- missing deduplicated chunks;
- stale roots selected after interrupted publication;
- compaction deleting reachable content;
- public content hashes leaking equality of confidential data.

### Acceptance questions

- Can readers locate one object with bounded memory and logarithmic or comparable index work?
- Can directories and indexes be paged rather than materialized entirely?
- Are reachability and compaction rules deterministic and auditable?
- Can integrity be checked across deduplicated references without trusting the index?
- Can a deployment disable public content addressing for privacy?

---

## UC-03 — Analytical table with selective reads

### Scenario

A data engineer stores an event table for analytical queries. Readers usually request a subset of columns and row groups based on statistics or predicates.

### Scale

- Logical size: 1 GB to 20 TB.
- Rows: 10 million to 100 billion.
- Columns: 10 to 10,000.
- Row groups: hundreds to millions.

### Access pattern

- Read schema and statistics first.
- Select a small number of columns.
- Skip row groups using min/max statistics, dictionaries, or Bloom filters.
- Stream selected column chunks into vectorized processing.
- Read over local disk and remote range requests.

### Mutability

Usually immutable partitions with append-only additions. Schema evolves over time; old partitions remain readable.

### Trust boundaries

Files may be supplied by external partners. Statistics and indexes are untrusted accelerators and may be maliciously inconsistent with data.

### Failure modes

- forged statistics causing incorrect query results;
- huge dictionary allocation;
- deeply nested schema expansion;
- incompatible logical-type interpretation;
- integer overflow in page and row counts;
- one column codec blocking unrelated columns;
- schema evolution losing unknown fields.

### Acceptance questions

- Can selected columns be read without touching unrelated payloads?
- Can filters use indexes without making them authoritative?
- Are row counts, null counts, and logical types unambiguous across languages?
- Can an old reader safely preserve or skip new optional columns?
- Do compression and encoding choices remain profile-level rather than mandatory core complexity?

---

## UC-04 — Interrupted append-only sensor capture

### Scenario

A device records timestamped sensor frames continuously to local storage while power loss, process termination, or media removal may occur at any byte boundary.

### Scale

- Capture duration: minutes to months.
- Write rate: 10 KB/s to 500 MB/s.
- Chunk count: thousands to billions.
- Individual frames: tens of bytes to several megabytes.

### Access pattern

- Sequential streaming writes without seeking.
- Periodic checkpoints.
- Sequential playback and time-range seeking after finalization.
- Recovery of the latest complete checkpoint after interruption.

### Mutability

Append-only during capture. Optional final indexing and compaction after capture ends.

### Trust boundaries

The writer may be trusted, but storage may tear or truncate writes. Recovered files may later be processed by software that treats metadata as untrusted.

### Failure modes

- partial chunk header or payload;
- footer written but referenced root incomplete;
- stale valid footer after newer partial append;
- clock discontinuity or duplicate timestamps;
- unbounded search for a checkpoint;
- false recovery from payload bytes resembling a footer;
- replay tools silently treating salvaged data as fully valid.

### Acceptance questions

- Does every interruption leave an earlier valid checkpoint unambiguous?
- Can recovery work with bounded backward scanning or explicit checkpoint discovery?
- Can streaming writers operate without buffering the whole object directory?
- Can recovered and uncommitted data be reported separately?
- Can finalization add efficient indexes without rewriting all payloads?

---

## UC-05 — Signed document package with private attachments

### Scenario

A document package contains a public report, embedded resources, structured metadata, signatures, editing provenance, and attachments encrypted for selected recipients.

### Scale

- Total size: 500 KB to 5 GB.
- Objects: 20 to 100,000.
- Signatures and claims: 1 to several thousand.

### Access pattern

- Display public content without private keys.
- Verify selected signatures and provenance claims.
- Decrypt only authorized attachments.
- Copy or forward the package while preserving unknown claims.
- Add a countersignature or later provenance event append-only.

### Mutability

Public content may be immutable after signing, while new signatures and provenance claims are appended.

### Trust boundaries

Authors, signers, recipients, certificate issuers, rendering software, and provenance producers have different trust levels. A valid signature does not automatically make content safe or true.

### Failure modes

- signature wrapping or ambiguous signed scope;
- unsigned manifest redirecting a signature to different content;
- algorithm or key confusion;
- metadata revealing private attachment names, sizes, or recipients;
- replayed or reordered provenance claims;
- unsupported claims being presented as verified;
- active content or external references triggering execution or retrieval.

### Acceptance questions

- Is every signature’s covered scope exact and canonical?
- Can public and private objects coexist without implying metadata confidentiality?
- Can a verifier distinguish integrity, signer authenticity, trust, and provenance assertion?
- Can unknown claims be preserved without being trusted?
- Can additional signatures be appended without invalidating earlier signed content unnecessarily?

---

## UC-06 — Metadata-confidential encrypted dataset

### Scenario

A regulated dataset must conceal not only payloads but also object names, types, counts, relationships, indexes, and statistics from anyone without authorization.

### Scale

- Total size: 100 MB to 10 TB.
- Objects: thousands to millions.
- Recipients: one to thousands, with key rotation over time.

### Access pattern

- Authenticate enough bootstrap information to choose a recipient key.
- Decrypt a protected directory or manifest.
- Perform selected random reads after authorization.
- Rotate recipient access without rewriting all payloads where feasible.

### Mutability

Append-only updates, recipient changes, and eventual compaction.

### Trust boundaries

Storage providers and transport are untrusted. Authorized readers may have access to different subsets. File size and access timing may remain observable.

### Failure modes

- public directory leaking object structure;
- nonce reuse during append;
- recipient-list disclosure;
- unauthenticated length or offset fields guiding reads;
- rollback to an older authorized root;
- equality leakage through public content hashes or deduplication;
- accidental mixed public/private indexing.

### Acceptance questions

- Which minimum bootstrap fields must remain public?
- Can protected discovery remain random-access without exposing plaintext indexes?
- Are nonce, key, and recipient identifiers unambiguous?
- Can rollback protection be layered without requiring a global online service?
- Does documentation clearly state residual leakage such as total size and access patterns?

---

## UC-07 — Damaged file with a valid earlier checkpoint

### Scenario

A file is truncated, partially overwritten, or has isolated corrupt regions, but an earlier committed snapshot remains intact.

### Scale

- Any file size from kilobytes to terabytes.
- Damage may affect the tail, middle regions, indexes, or selected payloads.

### Access pattern

- Strict validation first.
- Discovery of valid historical roots.
- Diagnostic inventory of damaged and unreachable regions.
- Optional salvage into a new file.

### Mutability

The damaged source is read-only. Repair writes a separate output by default.

### Trust boundaries

Damage may be accidental or adversarial. Recovery hints, indexes, and footer-like byte patterns cannot be trusted merely because they are plausible.

### Failure modes

- attacker-selected stale root presented as latest;
- overlapping regions interpreted differently by tools;
- salvage accepting unverified payloads as authoritative;
- repair modifying the only evidence copy;
- excessive scanning or diagnostic output;
- recovered references escaping the validated snapshot.

### Acceptance questions

- Can strict mode distinguish invalid from unsupported and recoverable?
- Can recovery enumerate candidate roots with confidence levels and evidence?
- Does salvage produce a new identity and explicit loss report?
- Are scanning, allocations, and diagnostic counts bounded?
- Can an application require freshness information external to the file when rollback matters?

---

## UC-08 — Core reader without profile support

### Scenario

A generic inspection tool receives a valid file using a profile it does not implement.

### Scale

- Any size and object count within configured limits.

### Access pattern

- Identify core version and experimental epoch.
- Read bootstrap metadata and active root.
- Enumerate required, optional, and advisory capabilities.
- List object identities, types, stored lengths, dependencies, and integrity status.
- Copy or extract opaque stored payloads where safe.

### Mutability

Read-only inspection or byte-preserving copy into another container operation.

### Trust boundaries

The profile and objects are untrusted. The generic reader must not execute embedded code or automatically fetch external resources.

### Failure modes

- reader guesses semantics for unknown object types;
- unknown required capability is ignored;
- copying drops unknown metadata needed for future interpretation;
- profile-specific transform is invoked accidentally;
- diagnostic output claims semantic validity it cannot establish.

### Acceptance questions

- Can the reader explain exactly what it does and does not understand?
- Can unknown optional structures be skipped and preserved safely?
- Does unknown required behavior fail closed for the affected scope?
- Can the core inventory be produced without profile libraries?
- Are opaque extraction and logical decoding clearly distinguished?

---

## UC-09 — Schema evolution across old and new readers

### Scenario

A profile evolves an object schema by adding fields, new logical types, and eventually a replacement representation while historical files and older readers remain in use.

### Scale

- Small metadata objects through large tabular schemas.
- Tens to tens of thousands of fields across nested structures.

### Access pattern

- New readers consume old files.
- Old readers inspect or preserve new files.
- Gateways read and rewrite files without understanding every extension.
- Compatibility checks occur before expensive payload reads.

### Mutability

Schemas and objects evolve across snapshots and files. Some migrations are lazy or profile-specific.

### Trust boundaries

Schema definitions may be embedded, externally identified, or supplied by another party. They are data, not executable code.

### Failure modes

- field-number or name reuse changes meaning;
- old writer drops unknown fields on rewrite;
- recursive schema references exhaust resources;
- incompatible default values produce silent semantic changes;
- logical type changes while physical bytes remain parseable;
- external schema identifier resolves to different content.

### Acceptance questions

- Are backward, forward, and full compatibility defined separately?
- Can unknown fields be preserved when the schema system supports it?
- Are schema identities content-bound or otherwise protected from substitution?
- Can readers reject incompatible logical types before interpreting payloads?
- Are migrations declarative and non-executable by default?

---

## UC-10 — Adversarial resource-exhaustion file

### Scenario

An attacker constructs a small or moderately sized file intended to consume extreme CPU, memory, storage, recursion depth, network activity, or diagnostics in a conforming reader.

### Scale

- Stored size: 1 byte to several gigabytes.
- Claimed logical size: up to numeric limits.
- Object graph: dense, cyclic, deeply nested, or fan-out heavy.

### Access pattern

Any operation, including identification, inventory, verification, extraction, schema loading, index lookup, recovery, or text conversion.

### Mutability

Not relevant; the file is hostile input.

### Trust boundaries

Every byte, identifier, length, index, dependency, transform, schema, signature, and external reference is attacker-controlled.

### Failure modes

- integer wraparound and allocation truncation;
- decompression bomb;
- cyclic dependencies or recursive aliases;
- quadratic or exponential index traversal;
- enormous maps, dictionaries, strings, or diagnostics;
- repeated digest or signature work;
- automatic network retrieval;
- parser disagreement exploitable across trust boundaries.

### Acceptance questions

- Can all major work dimensions be bounded by caller policy?
- Do failures return structured errors without panics or partial trust upgrades?
- Are limits enforced before allocation and transform execution?
- Can verification budgets distinguish bytes hashed from logical bytes expanded?
- Do strict and diagnostic modes agree on validity?

---

## 3. Cross-case tensions

The corpus intentionally exposes conflicting goals:

| Tension | Cases |
|---|---|
| Small metadata overhead vs. huge object counts | UC-01, UC-02 |
| Streaming publication vs. immediate random access | UC-04, UC-03 |
| Public deduplication vs. equality confidentiality | UC-02, UC-06 |
| Append-only history vs. compact storage | UC-02, UC-04, UC-07 |
| Rich provenance vs. privacy | UC-05, UC-06 |
| Generic opaque handling vs. profile correctness | UC-08, UC-09 |
| Fast indexes vs. untrusted accelerators | UC-02, UC-03, UC-07 |
| Recovery convenience vs. rollback resistance | UC-04, UC-06, UC-07 |
| Extensibility vs. a small safe parser | UC-08, UC-09, UC-10 |

A proposal that optimizes one side must state its effect on the other.

## 4. Phase 0 review checklist

A use case is ready for Phase 0 review when:

- its scale and access pattern are concrete;
- trust boundaries identify who controls the bytes and metadata;
- at least three realistic failure modes are recorded;
- success can be measured or tested;
- the scenario does not assume an undecided byte layout;
- conflicts with other scenarios are visible.

Open issues should track evidence gaps, candidate datasets, benchmark generators, and proposed additional workloads.
