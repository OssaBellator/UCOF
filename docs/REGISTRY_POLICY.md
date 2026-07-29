# UCOF Registry Policy

## 1. Purpose

UCOF registries assign stable identifiers to interoperable features without requiring every extension to change the core specification version.

This policy exists to prevent identifier collisions, semantic reuse, premature allocation, and implementation-specific behavior from becoming permanent by accident.

## 2. Planned registries

The project expects separate registries for at least:

- capabilities;
- core and profile object types;
- chunk or payload encodings;
- transforms and transform parameters;
- digest algorithms;
- signature and encryption suites;
- schema languages and logical types;
- profiles;
- provenance claim types;
- external-reference schemes;
- diagnostic or conformance error codes where stable cross-implementation identifiers are useful.

A Format Change Proposal may establish, split, or retire a registry.

## 3. Identifier classes

### 3.1 Permanent

Allocated through an accepted FCP for interoperable long-lived use. Permanent identifiers are never reassigned to incompatible semantics.

### 3.2 Experimental

Reserved for prototypes and pre-stable wire epochs. Experimental identifiers may change or collide outside their declared scope and must not be presented as durable interoperability contracts.

### 3.3 Private use

Reserved for closed deployments or local experiments that do not require global coordination. Private-use values must not appear in files claiming general interoperability unless a profile defines the private agreement explicitly.

### 3.4 Reserved

Set aside for future use, framing sentinels, invalid values, or allocation strategy. Reserved values must not be emitted except for their stated purpose.

The concrete numeric ranges and textual namespace rules will be selected through an FCP after Phase 1 establishes identifier widths and encoding constraints.

## 4. Allocation prerequisites

A permanent identifier may be allocated only when:

1. the related FCP is accepted;
2. the semantic definition is precise enough for independent implementation;
3. required and optional behavior is explicit;
4. compatibility and security impact are documented;
5. canonicalization or cryptographic scope is defined where relevant;
6. required test vectors or references exist;
7. the identifier does not duplicate an existing semantic entry;
8. ownership and change control are clear;
9. deprecation and failure behavior are defined;
10. the allocation does not depend on unpublished proprietary information.

Prototype popularity alone is not sufficient.

## 5. Registry entry fields

Each permanent entry should contain:

- identifier value;
- short name;
- full name;
- registry category;
- status;
- defining specification or accepted FCP;
- required capability classification, if applicable;
- parameters and canonical encoding rules;
- security and privacy considerations;
- contact or change authority;
- first assigned revision;
- deprecation or replacement information;
- notes on implementation availability and test vectors.

## 6. Status values

Registry entries may be:

- **Active** — suitable for new interoperable files.
- **Provisional** — accepted but awaiting specified implementation or test evidence; use may be restricted.
- **Deprecated** — readable for compatibility but discouraged for new output.
- **Withdrawn** — unsafe, ambiguous, or never completed; writers must not emit it except for explicit legacy testing.
- **Reserved** — unavailable for general allocation.

Changing status requires a recorded rationale. A security emergency may immediately move an entry to Withdrawn, followed by public retrospective documentation.

## 7. Immutability and semantic stability

An assigned identifier must never be silently redefined.

Compatible clarifications may update an entry when they do not change valid interpretation. Any semantic change that could alter parsing, canonical identity, signed scope, security properties, or application meaning requires a new identifier or an explicit versioned mechanism approved by FCP.

Typos in names may be corrected, but historical aliases should be recorded when software could depend on them.

## 8. Allocation strategy

Until numeric widths and wire representation are accepted, the project will not allocate production numbers.

The eventual strategy should consider:

- dense values for common mandatory features;
- ranges for experimental and private use;
- registry exhaustion;
- compact encoding cost;
- human transcription and diagnostic readability;
- collision resistance for decentralized schema or profile identifiers;
- whether textual names, numeric codes, or both are authoritative;
- algorithm agility and deprecation.

The project should avoid assigning central numeric codes to concepts that are better represented by content-derived or namespaced identifiers.

## 9. Vendor and organization extensions

Vendor-specific extensions must use an approved namespacing mechanism or private-use range. A vendor name does not by itself grant permanent ownership of a global numeric range.

A vendor extension may later become a standard extension through a new FCP. The standard identifier must not silently inherit incompatible historical behavior.

## 10. Schema and profile identifiers

Schema and profile identities may require more than a small numeric code because they can be decentralized and versioned.

An FCP defining their identity model must address:

- authority and collision handling;
- version versus content identity;
- mutable names and immutable definitions;
- offline resolution;
- external registry substitution;
- Unicode and case normalization;
- privacy and tracking effects;
- migration after ownership changes.

## 11. Cryptographic registries

Digest, signature, encryption, and key-encapsulation entries require especially strict review.

Entries must define:

- exact algorithm and parameter set;
- key, nonce, tag, and output lengths where applicable;
- domain separation and context binding;
- canonical input or associated data;
- prohibited parameter combinations;
- known security limitations;
- deprecation triggers;
- at least one authoritative primary specification;
- test vectors suitable for independent implementations.

Algorithm identifiers must never be inferred from output length or key type alone.

## 12. Transform registries

A transform entry must specify:

- input and output domains;
- parameter encoding;
- ordering constraints;
- deterministic versus non-deterministic behavior;
- expected and maximum expansion behavior;
- dictionary or external dependencies;
- streaming and random-access properties;
- failure behavior;
- security considerations;
- whether the transform changes logical identity or only stored representation.

## 13. Capability registry

A capability entry must state:

- the exact behavior it guards;
- whether it may be declared Required, Optional, or Advisory;
- the scope at which it applies;
- safe behavior when unsupported;
- interactions with profiles and other capabilities;
- whether support is reader-only, writer-only, or both;
- conformance tests.

Capabilities must not be used as vague feature flags or as substitutes for precise semantics.

## 14. Deprecation and withdrawal

Deprecation should include:

- reason;
- date or registry revision;
- affected versions and profiles;
- replacement identifier, if any;
- writer requirements;
- reader compatibility expectations;
- migration guidance;
- security consequences of continued support.

Withdrawal is appropriate when an entry is dangerously ambiguous, cryptographically broken, impossible to implement interoperably, or allocated but never completed.

Permanent identifier values remain reserved after deprecation or withdrawal.

## 15. Registry change process

Registry changes occur through reviewed pull requests that reference an accepted FCP or an allowed editorial correction.

A registry update must be atomic with or follow the defining specification material. It must not allocate identifiers ahead of proposal acceptance to “save” values.

## 16. Auditing and publication

Registries should be machine-readable and rendered into human-readable documentation once the first permanent allocations exist.

Continuous integration should eventually verify:

- unique identifiers and names;
- valid status transitions;
- resolvable defining references;
- required fields by registry type;
- no use of reserved ranges;
- no reassignment of historical values;
- synchronization between machine-readable and rendered forms.

## 17. Phase 0 decision

During Phase 0:

- no permanent numeric identifiers are allocated;
- examples use symbolic names or clearly marked experimental values;
- `UCOF-EXP-####` identifies incompatible experimental wire epochs;
- initial registry structure remains a design input for Phase 1;
- registry ranges and serialized identifier widths are open decisions.
