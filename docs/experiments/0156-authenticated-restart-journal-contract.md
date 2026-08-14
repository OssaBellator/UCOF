# Experiment 0156 — authenticated restart/discard journal contract

**Status:** non-normative Phase 3 contract evidence; **no production journal-authentication or durability claim**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0154–0155

## Purpose

Experiment 0155 defines the private-record AEAD integration boundary and requires an authenticated nonce checkpoint before restart can resume private-stage nonce allocation. That leaves a larger authority question: what exact state may a restart trust, what private artifacts does it govern, and when is destructive cleanup forbidden because publication may already have occurred?

This experiment makes that restart/discard authority model executable.

The model defines a canonical private journal payload, an authentication boundary, generation and nonce rollback floors, bounded artifact inventory, and a publication-aware authority state machine. The test authenticator is deliberately non-production and leaves journal plaintext visible. Therefore this experiment does not claim a durable or cryptographically authenticated journal implementation.

## Journal identity and framing

Each journal is bound to:

- a non-zero 16-byte operation ID;
- a non-zero 16-byte key ID;
- a monotonically increasing journal generation;
- the next private-stage nonce counter, where `None` represents exhausted nonce space;
- one publication/construction state;
- a bounded private-artifact inventory.

The private journal uses test-only framing:

- magic `UCOFJR01`;
- version 1;
- 96-byte canonical header;
- 64-byte canonical artifact records;
- exact-end decoding;
- zero-required reserved bytes.

The journal framing is private writer state and has no UCOF wire compatibility meaning.

## Private artifact inventory

The modeled artifact kinds are:

- sorter run;
- sorted source-descriptor stage;
- locator stage;
- page-reference stage;
- private final output artifact.

Each inventory entry binds:

- artifact kind;
- segment ID;
- non-zero byte length;
- SHA-256 digest.

Inventory length is caller-bounded. Duplicate `(kind, segment_id)` identities are rejected.

This is authority metadata only; Experiment 0156 does not yet prove descriptor-relative filesystem lookup, encrypted artifact contents, or durable artifact creation.

## Journal states

The model uses these states:

1. `Prepared`;
2. `Constructing`;
3. `PrivateSynced`;
4. `LinkIndeterminate`;
5. `ParentSyncIndeterminate`;
6. `Durable`;
7. `Discarded`.

Same-state checkpoints are allowed only for `Prepared`, `Constructing`, and `PrivateSynced`, so long-running work may advance generation/nonce state without pretending publication authority changed.

Allowed forward transitions are deliberately narrow:

- `Prepared -> Constructing | Discarded`;
- `Constructing -> PrivateSynced | Discarded`;
- `PrivateSynced -> LinkIndeterminate | ParentSyncIndeterminate | Durable | Discarded`;
- `LinkIndeterminate -> Durable`;
- `ParentSyncIndeterminate -> Durable`.

State regression is rejected.

## Recovery authority

Journal states map to explicit recovery authority:

- `Prepared`, `Constructing`, `PrivateSynced` -> `ResumeOrDiscardPrivate`;
- `LinkIndeterminate`, `ParentSyncIndeterminate` -> `ResolvePublication`;
- `Durable` -> `CleanupDurablePrivate`;
- `Discarded` -> `TerminalDiscarded`.

This distinction is critical. A restart observing an indeterminate publication state is **not** authorized to delete private state merely because construction is no longer active. It must first resolve publication authority.

## Indeterminate publication is non-destructive

Two regressions cover the publication-uncertainty cases.

For `LinkIndeterminate` and `ParentSyncIndeterminate`, the journal must:

- report `ResolvePublication` authority;
- reject transition to `Discarded`;
- retain the artifact inventory required to investigate or complete publication;
- allow resolution to `Durable`.

This matches the staged-publication evidence from Experiment 0154: a possible link or a failed parent-directory sync must not be converted into a generic construction failure followed by destructive cleanup.

## Generation rollback floor

Every accepted checkpoint advances generation using checked arithmetic. Generation wrap is rejected.

Authenticated loading accepts a caller-supplied minimum generation. A journal whose authenticated generation is below that floor is rejected as `GenerationRollback`.

This creates an executable rollback-detection boundary for a future durable journal backend. The experiment does not yet specify where the trusted generation floor itself is durably anchored.

## Nonce rollback floor

The journal carries the private-stage `next_nonce` state from Experiment 0155.

Checkpoint transitions permit:

- numeric counter -> same or larger numeric counter;
- numeric counter -> exhausted state;
- exhausted -> exhausted.

They reject:

- numeric counter rollback;
- exhausted state -> any numeric counter.

Authenticated loading likewise accepts a minimum nonce floor and rejects an older authenticated journal as `NonceRollback`.

A dedicated regression proves that an exhausted checkpoint cannot resurrect counter zero.

## Authenticated load boundary

`open_journal` performs these checks in order:

1. authenticate/open through a `JournalAuthenticator` boundary;
2. exact-end canonical decode;
3. expected operation ID;
4. expected key ID;
5. minimum generation floor;
6. minimum nonce floor;
7. maximum artifact count.

Foreign operation and key substitution are distinct failures.

## Adversarial evidence

The test suite requires fail-closed behavior for:

- journal-content tampering;
- truncation;
- foreign operation identity;
- foreign key identity;
- generation rollback;
- nonce rollback;
- state regression;
- destructive discard from either indeterminate publication state;
- duplicate artifact identity;
- artifact-count limit overflow;
- trailing bytes;
- non-zero reserved bytes;
- nonce resurrection after exhaustion.

It also proves that same-state construction checkpoints advance generation while preserving construction authority.

## Explicit non-production authenticator

The test authenticator copies journal plaintext unchanged and appends a deterministic SHA-256-based plumbing tag.

A regression explicitly verifies that the journal header remains visible in the sealed test bytes.

This is intentional. The test authenticator exists only to make tamper-detection and authentication-boundary plumbing executable. It is not a production MAC, AEAD, encrypted journal, or durability mechanism.

## Verification

Implementation head `278ce2dd53d96da0dc5a5a0c34c50256254f86d3` is green on the decisive implementation gates:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0156 rollback/authority/inventory cases and the Experiment 0155 private-stage crypto-contract tests;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The repository's longer policy/parser/vector/framing replay continues after those gates and provides broader regression confidence rather than changing the restart-authority result.

## Critical remaining nonce-crash gap

A journal containing `next_nonce` is **not by itself sufficient** to guarantee nonce uniqueness across process crashes.

If a writer:

1. reads authenticated journal counter `N`;
2. uses nonce `N` for private ciphertext;
3. crashes before durably committing journal counter `N + 1`;
4. restarts from the old authenticated journal;

then nonce `N` can be reused under the same key/prefix.

Therefore production code must not rely on post-use counter checkpointing.

The conservative next design is a **durably committed nonce lease/high-water mark**: reserve a bounded counter range in the authenticated journal and sync that reservation before any nonce in the range is used. On restart, the writer starts at the committed high-water mark and discards any unused counters from the previous lease. Wasting nonces is acceptable; reusing one is not.

## What remains open

Experiment 0156 does not close the restart requirement in issue #11. Still required are:

- real authenticated/encrypted journal storage;
- crash-safe journal replacement and directory durability;
- a trusted rollback anchor or equivalent generation policy;
- crash-safe nonce leases/reservations;
- integration between the journal inventory and real hardened filesystem handles;
- bounded stale-operation discovery, quarantine, and cleanup;
- production key lifecycle and nonce-prefix provenance;
- physical power-loss/filesystem qualification.

## Next executable slice

Experiment 0157 should model nonce leases and every crash cut around lease reservation, durable commit, nonce use, lease exhaustion, and restart. It should prove that no counter can be reused after restart, that unused counters in a committed lease are intentionally abandoned, that leases are bounded and disjoint, and that exhaustion cannot wrap.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.