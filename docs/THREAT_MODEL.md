# UCOF Initial Threat Model

## 1. Status and scope

This is the Phase 0 threat model for UCOF. It defines security and privacy requirements before the wire format and reference implementation exist.

It covers:

- core framing and discovery;
- object and chunk relationships;
- metadata, schemas, directories, and indexes;
- transform pipelines and compression;
- append-only snapshots, recovery, and compaction;
- digests, signatures, provenance, and encryption;
- generic readers, profile-aware readers, writers, CLI tools, and converters;
- local files and remote range-readable storage.

It does not claim that UCOF makes payload applications safe. A valid image, document, executable, model, or database object can still exploit the software that interprets it.

## 2. Security objectives

A conforming design and implementation should provide the following within explicitly declared scope.

### 2.1 Memory and arithmetic safety

Untrusted values must not cause memory corruption, unchecked integer behavior, invalid slicing, or allocation based on unvalidated arithmetic.

### 2.2 Bounded resource use

Callers must be able to bound bytes read, logical bytes decoded, allocation, object count, chunk count, nesting, recursion, dependency traversal, index work, transform expansion, cryptographic work, recovery scanning, and diagnostics.

### 2.3 Unambiguous interpretation

The same valid bytes should not produce materially different structure, canonical identity, signed scope, or active snapshot selection across conforming implementations.

### 2.4 Fail-closed capability handling

Unknown required behavior must not be guessed, ignored, or downgraded to optional.

### 2.5 Integrity separation

Readers must distinguish accidental-corruption checks, cryptographic digests, signatures, trust decisions, and provenance assertions.

### 2.6 Recovery without false validity

Recovery and salvage may identify useful data, but must not silently promote damaged, stale, or partially verified state to the active valid snapshot.

### 2.7 Confidentiality with explicit leakage

Encryption features must document what remains observable, including total size, access patterns, public bootstrap data, recipient information, equality, and unprotected indexes.

### 2.8 Non-executable parsing

Parsing metadata, schemas, profiles, transforms, and external references must not automatically execute embedded code or initiate network access.

## 3. Assets

Assets that may require protection include:

- process memory and control flow;
- CPU, memory, storage, file descriptors, and network budget;
- availability of services that ingest or inspect files;
- integrity of extracted or converted output;
- correctness of active snapshot selection;
- confidentiality of payloads and metadata;
- private keys, recipient lists, and trust configuration;
- authenticity and scope of signatures;
- accuracy of provenance presentation;
- durability of previous valid snapshots;
- identifier and registry stability;
- interoperability among implementations;
- the user’s original damaged file during repair operations.

## 4. Adversaries and failure sources

### 4.1 Malicious file producer

Controls every byte of a supplied file and attempts code execution, denial of service, data substitution, path escape, identity confusion, information disclosure, or parser disagreement.

### 4.2 Malicious storage or transport

Can truncate, reorder, replay, replace, omit, or corrupt ranges and may serve different bytes to different readers.

### 4.3 Malicious or compromised writer

Produces structurally plausible files with misleading manifests, indexes, signatures, provenance, recipient metadata, or history.

### 4.4 Curious storage provider

Does not modify data but attempts to infer content from sizes, names, hashes, relationships, access timing, or recipient metadata.

### 4.5 Buggy implementation

Accidentally emits or accepts ambiguous, non-canonical, inconsistent, or unsafe files.

### 4.6 Accidental damage

Power loss, torn writes, bit rot, incomplete transfers, disk-full errors, interrupted compaction, or operator mistakes.

### 4.7 Confused application

Correctly parses UCOF structures but incorrectly treats integrity as authenticity, a signed claim as truth, optional data as required, or salvaged data as valid.

## 5. Trust boundaries

Important boundaries include:

1. untrusted bytes entering a reader;
2. bootstrap discovery before an active root is authenticated;
3. indexes and directories directing reads to authoritative payloads;
4. stored bytes expanding into logical bytes through transforms;
5. schemas or profiles influencing semantic interpretation;
6. external references crossing from local parsing to retrieval;
7. cryptographic evidence crossing into application trust policy;
8. encrypted metadata becoming visible after key use;
9. extraction crossing from logical paths to a host filesystem;
10. repair tools writing new output from uncertain source evidence;
11. one implementation’s output becoming another implementation’s input;
12. historical roots competing with a claimed latest root.

## 6. Assumptions

The initial design assumes:

- readers can receive arbitrary hostile files;
- host applications provide resource policies appropriate to their environment;
- cryptographic algorithms are identified explicitly and can be deprecated;
- trusted keys and certificate policy are external to the core format;
- the format can carry external references but parsing alone never resolves them;
- stable security depends on canonical and signed scope definitions that are byte-precise;
- recovery cannot by itself prove freshness against rollback without external state or a trusted monotonic mechanism;
- confidentiality cannot hide total file size from an observer who stores the file;
- profile payloads may require separate sandboxing and content-specific security review.

The design must not assume:

- files fit in memory;
- offsets, lengths, counts, or indexes are honest;
- the last footer-like sequence is valid;
- a valid signature implies a trusted signer;
- encrypted payloads imply encrypted metadata;
- a digest algorithm is known from digest length;
- all implementations use the same integer width, Unicode behavior, floating-point behavior, or filesystem semantics;
- unsupported data can be safely ignored without a capability rule.

## 7. Threat catalogue

### TM-01 — Integer overflow and offset wraparound

**Attack:** Crafted offsets, lengths, counts, alignments, or additions overflow an implementation’s integer type or truncate during conversion.

**Impact:** Out-of-bounds access, overlapping validation bypass, incorrect allocations, file-region confusion, panic, or code execution in unsafe implementations.

**Required controls:**

- define serialized integer ranges independently of host word size;
- use checked arithmetic before every offset, length, count, alignment, and multiplication operation;
- reject values that cannot be represented safely by the implementation;
- validate `offset + length` without wrapping;
- test boundary values across 32-bit and 64-bit implementations;
- distinguish malformed input from configured resource-limit rejection.

### TM-02 — Forged lengths and overlapping ranges

**Attack:** Multiple structures claim the same bytes under conflicting meanings, extend beyond file bounds, point inside headers, or alias authenticated and unauthenticated regions.

**Impact:** Parser differentials, signature confusion, hidden payloads, arbitrary read patterns, and inconsistent extraction.

**Required controls:**

- define whether ranges may overlap and reject all undeclared overlap;
- validate stored regions against known file bounds when available;
- detect contradictory directory entries;
- make signed scope independent of unverified aliases;
- include malformed vectors for zero-length, adjacent, nested, duplicate, and partially overlapping ranges.

### TM-03 — Decompression and transform bombs

**Attack:** A small stored payload expands to extreme logical size or triggers excessive CPU through nested or adversarial transforms.

**Impact:** Memory exhaustion, disk exhaustion, CPU denial of service, and service instability.

**Required controls:**

- declare stored and expected logical lengths where meaningful;
- enforce per-transform and cumulative expansion budgets before and during decoding;
- cap transform count and pipeline depth;
- reject recursive transform dependencies;
- charge CPU-relevant work to a configurable budget where practical;
- permit metadata-only inspection without decoding payload transforms;
- authenticate transform declarations before trusting allocation guidance when cryptographic protection is expected.

### TM-04 — Deeply nested metadata

**Attack:** Metadata contains extreme nesting, huge maps, long strings, duplicate keys, or structures chosen for worst-case parser behavior.

**Impact:** Stack overflow, memory exhaustion, quadratic processing, canonicalization denial of service, and implementation disagreement.

**Required controls:**

- set explicit nesting, container-entry, string, byte-string, and total-metadata limits;
- use iterative parsing or guarded recursion;
- define duplicate-key handling unambiguously;
- canonicalize without unbounded in-memory sorting or document required bounds;
- reject indefinite or non-canonical forms in canonical scope if selected metadata encoding permits them otherwise.

### TM-05 — Recursive and cyclic object graphs

**Attack:** Objects, schemas, dictionaries, indexes, manifests, or external references form cycles or enormous fan-out graphs.

**Impact:** Infinite recursion, repeated work, stack exhaustion, verification loops, or unexpectedly large fetches.

**Required controls:**

- track visited identities in graph operations;
- define where cycles are forbidden, tolerated, or semantically meaningful;
- enforce dependency depth, edge count, and total-object budgets;
- avoid recursive default traversal APIs;
- make cycle errors stable across implementations.

### TM-06 — Malicious indexes and directories

**Attack:** An accelerator points to the wrong object, omits objects, forges statistics, creates traversal loops, or causes worst-case seeks.

**Impact:** Incorrect query results, data substitution, denial of service, information disclosure, or bypass of authoritative validation.

**Required controls:**

- define authoritative structures separately from accelerators;
- validate index-selected objects against identifiers and integrity metadata;
- allow indexes to be ignored or rebuilt where feasible;
- cap index depth, pages, comparisons, seeks, and result count;
- never use unverified statistics to change semantic query results;
- bind security-sensitive directories to the snapshot root when integrity is expected.

### TM-07 — Hash substitution and algorithm confusion

**Attack:** A digest is interpreted under the wrong algorithm, parameter set, canonicalization scope, or object context; weak and strong digests are mixed.

**Impact:** False integrity success, object substitution, cross-protocol collision, and deduplication errors.

**Required controls:**

- tag every digest with a registry identifier and defined parameters;
- include domain separation and object context where identities cross semantic domains;
- never infer algorithms from digest length;
- define canonical input bytes exactly;
- support algorithm deprecation and multi-digest migration without ambiguous precedence;
- keep non-cryptographic checksums distinct from security identities.

### TM-08 — Signature wrapping and ambiguous signed scope

**Attack:** A valid signature over one manifest, object, identity, or claim is presented as authenticating another or a broader container state.

**Impact:** Forged authorship, substituted content, misleading provenance, or authorization bypass.

**Required controls:**

- define signed bytes or canonical values exactly;
- include context, algorithm identifiers, profile, version, and scope in the signed message;
- prevent unsigned pointers from redirecting a signature to different content;
- make countersignature and multi-signature relations explicit;
- report cryptographic validity separately from trust and semantic claim validity;
- include wrapping and detached-object attack vectors in conformance tests.

### TM-09 — Encrypted metadata leakage

**Attack:** Payload encryption is correct, but public headers, directories, hashes, names, sizes, object types, relationships, statistics, recipients, or deduplication reveal sensitive facts.

**Impact:** Privacy breach, equality testing, traffic analysis, or exposure of regulated metadata.

**Required controls:**

- define public, protected, and encrypted metadata modes;
- document minimum unavoidable bootstrap leakage;
- permit protected directories and manifests;
- avoid public content hashes when equality leakage is unacceptable;
- authenticate all metadata that guides decryption and allocation;
- document recipient-list and file-size leakage;
- treat metadata confidentiality as independent from payload confidentiality.

### TM-10 — Nonce, key, and recipient confusion

**Attack:** Encryption reuses a nonce, applies the wrong key, confuses recipient identifiers, or accepts unauthenticated parameters during append or key rotation.

**Impact:** Plaintext recovery, forgery, recipient access errors, or permanent data loss.

**Required controls:**

- select misuse-resistant constructions where appropriate;
- specify nonce generation and uniqueness domains;
- bind keys, recipient entries, algorithms, and object context to authenticated data;
- prohibit implicit algorithm or key selection;
- define append and re-encryption workflows before standardization;
- test duplicate nonces, reordered recipients, and mixed algorithm suites.

### TM-11 — Parser differential behavior

**Attack:** The same bytes are treated differently by implementations because of integer, Unicode, floating-point, duplicate-key, trailing-data, canonicalization, or recovery ambiguities.

**Impact:** Signature bypass, policy mismatch, cache poisoning, data substitution, and interoperability failure.

**Required controls:**

- specify every byte range and error condition;
- publish valid and invalid vectors;
- require deterministic canonicalization vectors;
- perform differential testing across independent implementations;
- classify permissive diagnostic behavior separately from validity;
- avoid locale-, filesystem-, and host-language-dependent interpretation in the core.

### TM-12 — External-reference confusion

**Attack:** A reference resolves to unexpected content, a different scheme, a local file, a private network target, or mutable data that no longer matches its identity.

**Impact:** Server-side request forgery, local-file disclosure, substitution, tracking, and non-reproducible interpretation.

**Required controls:**

- parsing must never fetch automatically;
- retrieval requires explicit caller policy and scheme allow-listing;
- external content must be identity-bound when integrity matters;
- define base resolution without ambient working-directory dependence;
- cap redirects, bytes, time, and dependency depth;
- distinguish unavailable from invalid and unverified content;
- prevent references from silently escaping a package or sandbox.

### TM-13 — Unbounded dictionary or schema expansion

**Attack:** Shared dictionaries, schema imports, aliases, defaults, or logical types expand recursively or allocate extreme state.

**Impact:** CPU and memory exhaustion, semantic inconsistency, and parser disagreement.

**Required controls:**

- enforce schema size, import depth, symbol count, and expansion budgets;
- require stable identities for referenced definitions;
- forbid executable migration or validation code in the core path;
- detect cycles and duplicate definitions;
- allow structural inventory without loading full schema semantics;
- make external schema retrieval opt-in.

### TM-14 — Recovery selects stale or attacker-chosen roots

**Attack:** An attacker appends a plausible footer, replays an older valid root, hides a newer root, or exploits fallback logic after corruption.

**Impact:** Rollback, lost updates, presentation of revoked or superseded state, and false claims of latest validity.

**Required controls:**

- define active-root selection deterministically;
- authenticate root chains where integrity is expected;
- distinguish “latest found,” “latest valid in this byte sequence,” and “fresh according to external policy”;
- bound backward scanning and candidate count;
- never infer freshness solely from a timestamp supplied by the file;
- report recovered historical state explicitly;
- support external anti-rollback state when applications require it.

### TM-15 — Truncation and torn publication

**Attack or failure:** Writing stops at every possible byte boundary around chunks, directories, manifests, and footers.

**Impact:** Ambiguous active state, corrupted prior snapshot, or unsafe recovery.

**Required controls:**

- append new data before publishing a new root;
- make root publication independently detectable and bounded;
- preserve the prior valid root on incomplete append;
- test interruption at every byte boundary near commit structures;
- define trailing bytes and partial structures precisely;
- avoid in-place mutation of data required by the previous committed snapshot.

### TM-16 — Compaction and garbage-collection errors

**Attack or failure:** Reachability is computed from malicious, incomplete, or ambiguous roots; compaction drops required unknown data or changes signed semantics.

**Impact:** Permanent data loss, invalid signatures, broken references, and false equivalence claims.

**Required controls:**

- compact into a separate output by default;
- select source snapshots explicitly;
- preserve unknown required data or refuse compaction;
- verify output before publication;
- produce a report of retained and discarded history;
- assign new physical identity and document effects on instance and content identities;
- test interrupted compaction and insufficient-storage behavior.

### TM-17 — Extraction path and host-filesystem attacks

**Attack:** Archive or package objects contain absolute paths, `..`, reserved names, alternate data streams, device paths, case collisions, links, or permission metadata that escape or alter the destination.

**Impact:** Arbitrary file overwrite, privilege escalation, code execution, and data loss.

**Required controls:**

- profile specifications define path normalization independent of host filesystem;
- extraction tools use a confined destination and reject escape;
- links require explicit policy and safe target validation;
- collisions and platform-incompatible names are reported rather than guessed;
- permissions, ownership, and special files require opt-in restoration;
- extraction never executes installed content automatically.

### TM-18 — Diagnostic and error-channel abuse

**Attack:** A file triggers enormous numbers of warnings, embeds terminal control characters, leaks secret values into logs, or causes expensive pretty-printing.

**Impact:** Log injection, denial of service, secret disclosure, and operator confusion.

**Required controls:**

- cap diagnostic count and rendered value size;
- escape control characters and untrusted paths;
- separate machine-readable codes from localized messages;
- avoid logging decrypted content, keys, or full attacker-controlled payloads by default;
- preserve validity classification even when diagnostics are truncated.

### TM-19 — Canonicalization ambiguity

**Attack:** Multiple byte encodings map to the same apparent value or implementations normalize maps, integers, floats, Unicode, tags, or unknown fields differently.

**Impact:** Digest and signature mismatch, duplicate identities, cache poisoning, and cross-language disagreement.

**Required controls:**

- define a constrained canonical value model;
- specify ordering, integer widths, floating-point edge cases, Unicode treatment, duplicate handling, and tags;
- reject non-canonical forms in canonical identity scope or normalize before signing according to one exact algorithm;
- publish cross-language vectors including edge values;
- avoid canonicalization that depends on locale or platform libraries with divergent behavior.

### TM-20 — Provenance misrepresentation

**Attack:** Signed or unsigned history is reordered, omitted, replayed, selectively disclosed, or presented as objective truth.

**Impact:** Misleading users about origin, edits, custody, software, or authenticity.

**Required controls:**

- distinguish claim issuer, claim content, signature validity, and trust;
- bind claims to exact input and output identities;
- make ordering and previous-claim links explicit when asserted;
- permit a truthful “history incomplete or absent” state;
- avoid user interfaces that convert unsigned claims into verified history;
- document privacy risks of device, person, location, and software metadata.

## 8. Security requirements for reader APIs

Reader APIs should expose:

- immutable configuration containing resource limits;
- strict, diagnostic, and salvage operations as distinct entry points or modes;
- structured error categories;
- detected version or experimental epoch;
- supported and unsupported capabilities;
- validation status for bootstrap, root, directory, object, transform, digest, signature, and profile layers;
- bytes read and logical bytes produced where practical;
- no implicit network, filesystem extraction, key lookup, or code execution;
- cancellation and timeout integration where the host environment supports it.

Defaults for general-purpose tools should be conservative. Libraries must not hide unlimited behavior behind convenience APIs.

## 9. Security requirements for writer APIs

Writers should:

- validate all offsets, counts, identifiers, and transform output;
- prevent duplicate or conflicting identifiers in a committed scope;
- publish roots only after referenced required data is complete;
- use deterministic mode for fixtures and signed canonical content;
- make non-deterministic fields explicit;
- separate finalization from successful payload writes;
- avoid nonce reuse and ambiguous key selection;
- refuse to label unsupported semantics as optional;
- expose failure before replacing an existing destination;
- support writing to a temporary output and atomic host-level replacement where appropriate.

## 10. Cryptographic design rules

No cryptographic algorithm becomes mandatory or permanent merely because it is convenient in a prototype.

Any cryptographic proposal must define:

- algorithm identifier and parameter encoding;
- domain separation and context binding;
- exact input bytes or canonical values;
- key and recipient identification;
- failure behavior;
- algorithm-agility and deprecation path;
- interaction with streaming, random access, append, recovery, and compaction;
- metadata leakage;
- test vectors from independent implementations where feasible.

Custom cryptographic primitives are out of scope.

## 11. Privacy analysis requirements

Every proposal involving identifiers, indexes, provenance, signatures, external references, deduplication, encryption, or telemetry must state:

- what data is public;
- what equality or relationship information is exposed;
- whether stable identifiers enable tracking;
- what recipients or signers are revealed;
- what access patterns remain visible;
- what information survives compaction or redaction;
- whether a verifier must contact a network service;
- how users can omit, minimize, or encrypt sensitive metadata.

## 12. Validation strategy

The project should maintain:

- smallest-valid and boundary-value vectors;
- malformed framing and overlap vectors;
- truncation at every byte boundary near publication structures;
- cyclic and fan-out object graphs;
- nested metadata and dictionary bombs;
- compression and transform expansion cases;
- ambiguous canonicalization cases;
- malicious indexes and forged statistics;
- stale-root and footer-confusion cases;
- signature-wrapping and algorithm-confusion vectors;
- encryption nonce and recipient-confusion vectors;
- cross-language differential tests;
- fuzz targets for every parser boundary;
- corpus minimization and regression preservation.

Security tests should assert stable error category and resource-bound behavior, not only lack of crashes.

## 13. Residual risks and non-guarantees

Even a conforming UCOF implementation cannot guarantee:

- that payload content is safe to render or execute;
- that a signer is trustworthy;
- that a provenance claim is true;
- freshness against rollback without an external trust mechanism;
- secrecy of total size or access timing from the storage layer;
- availability of external references;
- successful interpretation of unknown required profiles or transforms;
- safe host-filesystem extraction without profile-aware policy;
- protection from compromised keys or malicious trusted software.

Applications must communicate these limits accurately.

## 14. Review triggers

This threat model must be reviewed when:

- Phase 1 selects framing and canonical metadata;
- compression or transform pipelines are introduced;
- signatures, provenance, or encryption are proposed;
- external references are enabled;
- the Archive or Table profile begins implementation;
- a parser differential is discovered;
- a security report changes an assumption;
- the project approaches a stable specification release.

## 15. Phase 0 open security decisions

The following remain intentionally unresolved:

- maximum bootstrap and footer search bounds;
- canonical metadata encoding and permitted subset;
- baseline digest and domain-separation scheme;
- whether directories are always root-authenticated;
- how protected metadata discovery works;
- whether content identities are public, private, or profile-controlled;
- recovery candidate ordering and freshness terminology;
- minimum required limits for a conforming implementation;
- external-reference identifier and retrieval model;
- signature and provenance claim envelope.

These must be resolved through Format Change Proposals with adversarial vectors, not hidden implementation choices.

## 16. Executable findings from UCOF-EXP-0001

The Phase 1 and Phase 2 implementations provide executable evidence for the following controls and limitations. These findings refine the initial threat catalogue but do not stabilize the experimental epoch.

### 16.1 Confirmed controls

- file-size policy is applied before structural parsing;
- offsets and lengths are checked before range construction and host-size conversion;
- record, payload, metadata, text, byte-string, nesting, container-item, allocation, logical-byte, read-byte, and diagnostic limits are caller controlled;
- unknown required capabilities fail closed for conforming interpretation while remaining visible to structural inventory;
- the restricted deterministic-CBOR subset rejects non-shortest arguments, indefinite forms, duplicate or unordered map keys, invalid UTF-8, negative integers, and floating-point values;
- directories are cross-checked against physical framing rather than treated as authoritative;
- strict validation uses exact-end footer discovery and does not silently fall back to recovery scanning;
- metadata-only inspection can skip multi-gigabyte virtual payload ranges while reading only bounded structural bytes;
- sequential validation hashes the committed prefix incrementally and publishes a verified commit event only after footer, directory, manifest, digest, and exact-end checks pass;
- strict random-access validation hashes payloads in bounded blocks under the same cumulative read budget as metadata inspection;
- streaming writers emit the footer only through explicit successful finalization and become terminal after source or sink failure;
- strict diagnosis remains invalid after integrity failure even when structural context is available;
- prefix salvage reports only complete in-bounds records and always labels its result `UnverifiedPrefix`.

### 16.2 Validation-order finding

A changed committed byte does not imply one universal error category. Framing mutations may fail structural checks before digest comparison. Payload mutations that preserve framing reach `DigestMismatch`. A record-identity mutation with a recomputed digest can reach directory cross-validation and fail as `DirectoryMismatch`.

Tests must therefore target and assert the intended validation layer. Implementations must not reorder checks in a way that exposes unsafe allocation or semantic use before required structural checks.

### 16.3 Footer and recovery finding

EXP-0001 stores footer fields outside the committed-prefix digest, so manifest identity, record count, directory location, and digest bytes require explicit structural and semantic checks. The current strict readers perform those checks.

A bounded 64 KiB backward-search region can contain thousands of attacker-selected footer-magic candidates. Normal validation must remain exact-end. Future recovery scanning must independently bound scan bytes, candidate count, validation work, and diagnostics, and must never upgrade a recovery candidate to strict validity without satisfying the active-root rules defined in Phase 3.

### 16.4 Scale finding

The flat EXP-0001 directory is not suitable for UC-02-scale archives. One million zero-byte objects require a lower bound of approximately 40 MB of record headers and about 52 MB of directory payload before application data. Raising parser limits does not solve this architecture failure. Phase 3 must use a paged or hierarchical lookup structure whose single-object lookup does not require materializing every entry.

### 16.5 Differential and fuzz evidence

- Rust and independent Python implementations agree on the valid and malformed shared corpus;
- supported primitive encodings and deterministic map order match pinned Ciborium 0.2.2, while UCOF intentionally rejects broader general-CBOR forms;
- property tests cover deterministic writer equivalence, truncation rejection, and payload mutation;
- dedicated fuzz targets cover full-file validation, canonical metadata, metadata inspection, prefix salvage, sequential reading, and writer round trips;
- fuzz targets compile and run bounded smoke campaigns in pull requests and scheduled campaigns weekly;
- the core library compiles for 32-bit little-endian and 64-bit big-endian targets, while serialized integers remain explicitly little-endian.

### 16.6 Residual risks

- SHA-256 in EXP-0001 provides integrity relative to stored footer data, not authenticity, signer trust, freshness, or rollback protection;
- strict random-access validation assumes a stable source view for one operation;
- the directory and writer ledger remain materialized and therefore bounded by object count and metadata limits;
- sequential payload events currently own their chunk buffers;
- salvage stops at the first fatal framing error and does not resynchronize;
- transforms, compression, schemas, signatures, provenance, encryption, external references, snapshots, and checkpoint recovery remain outside the experimental epoch;
- native dependencies introduced by future transforms require sanitizer-backed and supply-chain review beyond the current pure-Rust core.

The detailed Phase 1 evidence remains in `docs/security/EXP_0001_FINDINGS.md` and the FCP-0001 evidence appendix. Future experiments must update this section when they confirm, invalidate, or supersede these findings.

## 17. Executable findings from UCOF-EXP-0002 Candidate 1

Candidate 1 defines exact disposable bytes for authenticated pages, complete snapshots, append commits, source-based validation, recovery, verified history, lookup, repair, and caller-directed rewrite. It remains non-stable and Draft. Detailed evidence is recorded in `docs/security/EXP_0002_BYTE_FINDINGS.md`.

### 17.1 Serialized-structure controls

Rust and independent in-repository Python readers enforce exact magic, version, lengths, little-endian fields, zero flags, zero reserved bytes, zero page padding, checked range arithmetic, and an explicit SHA-256 algorithm identifier. Object, page, snapshot, and commit hashes use separate domain prefixes.

An authenticated outer digest does not replace inner validation. Layer-targeted cases recompute outer hashes and still reach rejection for malformed padding, forged child ranges, inconsistent levels, invalid parent links, object-header disagreement, physical overlap, and strict trailing bytes.

### 17.2 Directory, lookup, and page-identity findings

Candidate 1 pages require sorted unique leaf keys, sorted non-overlapping child ranges, exact child levels, exact 16 KiB lengths, complete-page digests, and cycle or repeated-offset rejection during traversal.

Full strict validation authenticates every reachable page and every referenced object. Targeted lookup instead authenticates the exact-end commit, active snapshot, one root-to-leaf path, and the selected object or absence result. It does not claim unrelated historical objects were rehashed.

A concrete page-reuse experiment produced a blocking negative result: Candidate 1 requires each page sequence to equal the active snapshot sequence. An unchanged historical page therefore fails when referenced by a later snapshot. Re-encoding the page changes its digest and propagates through every ancestor. Candidate 1 cannot provide exact historical directory-page reuse; a successor must replace page-sequence equality with independently implementable immutable-page or page-birth semantics.

The abstract persistent-tree model remains useful algorithm evidence, but it does not override the concrete byte-level rejection. Checkpoint-cadence evidence further shows that naive per-object path copying can write more metadata than one batched rebuild.

### 17.3 Bounded source and transport findings

Candidate 1 has bounded seekable-source implementations for targeted lookup, full strict validation, explicit recovery, and verified previous-footer history. They separately bound read operations, bytes read, maximum request size, pages, objects, bytes hashed, chain depth, scan work, candidate work, and returned results.

ADR-0013 defines a stable-view adapter for mutable or remote sources. A caller supplies a strong 32-byte token derived from storage identity and immutable version evidence. The adapter checks it before and after every length or range read and fails on any change. The token is transport evidence, not a UCOF field or digest.

Stable view is not freshness. A malicious source can consistently serve an older valid file and matching token. Whole-file rollback still requires external trusted state.

A localhost immutable HTTP Range experiment over an append file containing an unrelated 1 MiB historical payload measured seven requests and 33,610 transferred bytes for targeted lookup, versus 25 requests and 1,082,288 bytes for full strict validation. Lookup did not request the large historical payload; strict validation did. Localhost elapsed time is not a wide-area latency guarantee.

### 17.4 Publication, recovery, and history findings

Only a complete 160-byte footer at exact file end publishes the active snapshot. Every tested incomplete append fails strict latest validation. Strict mode never invokes recovery.

Recovery is separately requested and independently bounds suffix bytes, scan requests, footer-magic matches, candidate validations, cumulative reads spent on successful and failed candidates, chain depth, and returned results. Footer magic and previous-footer pointers have no authority without exact-prefix strict validation.

Verified history validates the active exact-end file and each linked ancestor as an exact-end prefix. It cross-checks previous-footer locators, parent snapshot digests, sequence increments, roots, and commit identity under cumulative-read and depth limits. Equal verified forks remain ambiguous rather than being silently selected.

### 17.5 Identity, repair, rewrite, and writer findings

ADR-0011 separates structural snapshot identity from file-instance commit identity. A deterministic repair may preserve an identical snapshot digest while publishing a different commit digest. This does not preserve the original file instance or byte-scoped signatures.

Repair and caller-directed rewrite accept only strictly verified complete sources, copy authenticated payload ranges, enforce object, copied-byte, and output-byte limits, require retained roots, validate generated output, and never claim automatic semantic dependency discovery.

A bounded external-sort experiment processed 200,003 exact 88-byte locator-shaped records using sub-megabyte spill runs, deterministic output across run sizes, exact spill/output accounting, and duplicate rejection. Integration still requires spill cleanup, confidentiality, descriptor, storage-exhaustion, and page-emission policy.

### 17.6 Differential, invalid-vector, fuzz, and portability evidence

- Rust and independent Python implementations produce byte-identical genesis, append, and multi-leaf vectors;
- thirteen pinned invalid and interrupted vectors carry strict-rejection and diagnostic-layer expectations;
- 21 layer-targeted adversarial cases exercise headers, objects, pages, padding, links, footers, exact-end state, and append truncation;
- 21 cargo-fuzz targets cover inherited byte paths, Phase 3 models, strict parsing, writers, lookup, source lookup, full source validation, recovery, rewrite, and source history;
- the workspace compiles at Rust 1.85, on 32-bit little-endian, and on 64-bit big-endian targets;
- all permanent workflows use read-only repository permissions.

Both implementations remain in one repository and may share a specification misunderstanding. Independent maintenance or review is still required.

### 17.7 Residual risks and open controls

- SHA-256 integrity is not authenticity, signer trust, confidentiality, provenance, or external freshness;
- a valid older whole file can be replayed without external trusted state;
- Candidate 1 page-sequence semantics prevent true historical page reuse;
- the 88-byte leaf entry dominates large-directory metadata cost;
- external sorting is not yet integrated with the writer or spill-security policy;
- retry, cancellation, deadline, and asynchronous transport behavior remain undefined;
- normative minimum resource limits and future-field preservation rules remain unresolved;
- semantic compaction requires profile, schema, or caller-supplied dependency information;
- transforms, compression, signatures, provenance, encryption, protected metadata, and external references remain outside Candidate 1.

FCP-0002 must not enter Review until the page-identity blocker, locator and identifier widths, writer integration, normative limits, preservation rules, transport behavior, independent evidence, external freshness, and substantive maintainer objections are resolved.
