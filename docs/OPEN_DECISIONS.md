# UCOF Open Decisions

## 1. Purpose

This document makes disputed or intentionally deferred requirements visible. An undecided item must not become normative merely because a prototype chooses one option.

Statuses:

- **Resolved** — recorded by repository policy, accepted proposal, or stable specification.
- **Provisional** — current working direction, subject to evidence and proposal review.
- **Open** — no preferred solution has been accepted.
- **Blocked** — depends on an earlier experiment or decision.

## 2. Phase 0 project decisions

| ID | Decision | Status | Current outcome or next action |
|---|---|---|---|
| P0-01 | Project license | Resolved | Existing repository license is MIT. |
| P0-02 | Draft specification license | Resolved | MIT applies repository-wide unless changed through an FCP. |
| P0-03 | Governance model | Resolved | Maintainer-led and consensus-seeking during the single-maintainer stage. |
| P0-04 | Normative decision mechanism | Resolved | FCPs for format behavior; ADRs for implementation-local choices. |
| P0-05 | Permanent identifier allocation | Resolved | No allocation before the defining FCP is accepted. |
| P0-06 | Software version scheme | Resolved | Semantic Versioning after releases begin. |
| P0-07 | Specification version scheme | Resolved | Independent `MAJOR.MINOR` core and profile versions. |
| P0-08 | Pre-stable file identification | Resolved | Monotonic `UCOF-EXP-####` experimental epochs. |
| P0-09 | Independent implementation before 1.0 | Resolved | Required; the second parser must not merely wrap the reference library. |
| P0-10 | Initial reference language | Provisional | Rust remains the recommended reference implementation language. Record an ADR when workspace implementation begins. |
| P0-11 | Initial profiles | Provisional | Archive and Table remain the first validation profiles. Confirm after Phase 1 core experiments. |
| P0-12 | Patent/governance structure for standardization | Open | Revisit before 1.0 or when multiple organizations depend on UCOF. |

## 3. Phase 1 core framing decisions

| ID | Decision | Status | Evidence required |
|---|---|---|---|
| C-01 | Byte order | Open | Annotated layouts, cross-language prototype, implementation simplicity analysis. |
| C-02 | Fixed bootstrap header size | Open | Truncation behavior, extension pressure, minimal parser complexity. |
| C-03 | Header magic and experimental epoch encoding | Open | False-positive analysis, future stable-version migration. |
| C-04 | Footer locator strategy | Open | Bounded discovery, trailing bytes, remote range reads, false-footer resistance. |
| C-05 | Fixed-width versus variable-width core integers | Open | File overhead, overflow safety, skip logic, cross-language implementation. |
| C-06 | Alignment and padding rules | Open | Memory mapping benefit versus ambiguity and overhead. |
| C-07 | Chunk framing and length semantics | Open | Streaming, truncation vectors, safe skipping, very large payload handling. |
| C-08 | Object and chunk identifier widths | Blocked | Depends on identity model and serialized integer choice. |
| C-09 | Minimum manifest fields | Open | Generic-reader use case and active-root verification. |
| C-10 | Primary directory representation | Open | Million-object scaling, bounded memory, remote access. |
| C-11 | Directory authority | Open | Recovery and malicious-index analysis; likely root-bound but reconstructibility must be precise. |
| C-12 | Active snapshot selection | Open | Interrupted append at every boundary, stale-root and rollback terminology. |
| C-13 | Trailing byte policy | Open | Appending, concatenation, false footer, and strict-versus-diagnostic behavior. |
| C-14 | Unknown object preservation rules | Open | Core-only copy prototype and capability model tests. |

## 4. Canonical metadata and identity decisions

| ID | Decision | Status | Evidence required |
|---|---|---|---|
| M-01 | Canonical metadata encoding | Provisional | Deterministic CBOR is the leading candidate; prove required semantics without custom ambiguity. |
| M-02 | Allowed canonical value subset | Open | Map ordering, tags, integers, floats, duplicate keys, indefinite forms, Unicode treatment. |
| M-03 | Floating-point representation | Open | Cross-language edge vectors including NaN, infinities, negative zero, and width minimization. |
| M-04 | Unicode normalization | Open | Decide whether strings are preserved as code points or normalized in selected identity scopes. |
| M-05 | Unknown field canonical preservation | Open | Round-trip experiments across old and new readers. |
| I-01 | Instance identifier model | Open | Mutation, copy, compaction, and privacy analysis. |
| I-02 | Content identity scope | Open | Canonical object boundaries, encrypted content, and deduplication requirements. |
| I-03 | Baseline digest algorithm | Open | Security lifetime, implementation availability, output size, streaming, and domain separation. |
| I-04 | Digest domain separation | Open | Cross-object and cross-protocol substitution tests. |
| I-05 | Public versus protected content identities | Open | UC-02 deduplication versus UC-06 equality confidentiality. |

## 5. Streaming, recovery, and mutation decisions

| ID | Decision | Status | Evidence required |
|---|---|---|---|
| S-01 | Stream-compatible object ordering | Open | Sequential reader/writer prototype with bounded buffering. |
| S-02 | Checkpoint representation | Open | Sensor-capture interruption tests and remote discovery cost. |
| S-03 | Root publication atomicity assumptions | Open | Host filesystem and object-storage models; specification must not assume more than bytes provide. |
| S-04 | Historical root chain | Open | Recovery, compaction, privacy, and signature impact. |
| S-05 | Recovery candidate ordering | Open | Stale-root attack analysis and deterministic diagnostics. |
| S-06 | Freshness and rollback terminology | Open | Define file-internal validity separately from external freshness policy. |
| S-07 | Compaction identity effects | Open | Instance identity, content identity, signatures, unknown-object preservation. |
| S-08 | In-place mutation | Provisional | Core favors append-only copy-on-write; database profiles may require constrained additional rules. |

## 6. Transform and schema decisions

| ID | Decision | Status | Evidence required |
|---|---|---|---|
| T-01 | Transform pipeline descriptor | Blocked | Phase 1 chunk and metadata decisions. |
| T-02 | Baseline compression codec | Provisional | Zstandard is a candidate; requires bounded decoder, dictionaries, and independent support analysis. |
| T-03 | Cumulative expansion accounting | Open | Nested transform and chunk-group experiments. |
| T-04 | Transform determinism declarations | Open | Interaction with content identity and reproducible output. |
| SC-01 | Schema descriptor envelope | Blocked | Canonical metadata and object reference model. |
| SC-02 | Embedded versus external schema identity | Open | Offline use, substitution resistance, and schema evolution cases. |
| SC-03 | Core logical types | Open | Keep minimal; require use-case evidence before allocation. |
| SC-04 | Diagnostic text syntax | Blocked | Canonical value model must be accepted first. |

## 7. Cryptography, provenance, and privacy decisions

| ID | Decision | Status | Evidence required |
|---|---|---|---|
| CR-01 | Signature envelope | Blocked | Canonical identity and scope model. |
| CR-02 | Trust-policy boundary | Provisional | Core carries evidence; applications decide trusted keys and claims. |
| CR-03 | Provenance claim chain | Open | Omission, replay, ordering, selective disclosure, and privacy analysis. |
| CR-04 | Encryption granularity | Open | Object versus chunk access, recipient changes, random access. |
| CR-05 | Protected directory discovery | Open | Minimum public bootstrap and range-read feasibility. |
| CR-06 | Nonce management for append | Open | Selected authenticated-encryption construction and root publication model. |
| CR-07 | Recipient privacy | Open | Public versus encrypted recipient tables and discovery mechanism. |
| CR-08 | Anti-rollback integration | Open | Keep external-policy support without requiring an online global service. |

## 8. External references and networking

| ID | Decision | Status | Evidence required |
|---|---|---|---|
| E-01 | External-reference support in core or extension | Open | Portability, security, and offline behavior analysis. |
| E-02 | Reference identity requirement | Open | Mutable URLs, content addressing, and failure semantics. |
| E-03 | URI or scheme model | Open | SSRF, local-file access, normalization, and profile constraints. |
| E-04 | Retrieval API boundary | Resolved | Parsing never triggers retrieval; caller policy must opt in. |

## 9. Resource-limit conformance decisions

| ID | Decision | Status | Evidence required |
|---|---|---|---|
| L-01 | Mandatory limit categories | Provisional | Categories listed in the threat model; exact API and minimums remain open. |
| L-02 | Normative minimum supported values | Open | Representative corpus and constrained-device analysis. |
| L-03 | Work budgeting for indexes and crypto | Open | Adversarial benchmarks and API design. |
| L-04 | Diagnostic truncation behavior | Open | Stable machine-readable errors and log-safety tests. |
| L-05 | 32-bit implementation support | Open | Prototype parser and range policy. |

## 10. Resolution rule

A technical item moves to Resolved only when its authority is linked: accepted FCP, accepted ADR for implementation-only choices, registry entry, released specification, or explicit repository policy.

Pull requests that depend on an Open item must either remain experimental or include the proposal needed to resolve it.
