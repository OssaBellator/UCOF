# Experiment 0164 — bounded cleanup restart identity inventory

**Status:** non-normative Phase 3 restart-classification evidence; filesystem enumeration remains modeled input  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiment 0163

## Purpose

Experiment 0163 defines crash-authoritative cleanup ordering but receives restart pathname/identity observations from the caller. This experiment removes the unbounded-absence assumption by defining a streaming, bounded identity inventory whose output is safe to feed into restart classification.

The central rule is conservative:

> artifact absence is proven only after a complete scan with no unreadable metadata and no matching identity anywhere else.

Entry-limit, metadata-byte-limit, accounting-overflow, or unreadable-entry cases cannot prove absence.

## Bounds

The inventory requires nonzero:

- maximum entries scanned;
- maximum metadata bytes charged.

Each candidate is charged before it is classified. Checked accounting is used throughout.

A 1,000,000-entry iterator under a 32-entry cap scans exactly 32 entries, marks the scan truncated, and returns `MissingScanTruncated` rather than absence.

## Decisive positive evidence

Two observations may terminate classification immediately:

- the expected name resolves to the exact expected identity -> `ExactIdentity`;
- the expected name resolves to a different identity -> `DifferentIdentity`.

A matching expected identity found under another name is positive evidence that private state survives. That remains `MissingMatchingIdentityElsewhere` even if a later entry bound truncates the scan.

## Absence proof

`MissingNoMatchingIdentityCompleteScan` is emitted only when all of these hold:

- iteration completed naturally;
- no scan bound was hit;
- no metadata accounting overflow occurred;
- no entry was unreadable;
- the expected name was not found;
- the expected identity was not found under another name.

A metadata-byte cap that admits only the first of two entries returns `MissingScanTruncated`, not absence.

## Unreadable metadata

An unreadable non-expected entry prevents absence proof because it may conceal the expected identity.

Unreadable expected-name metadata receives its own explicit `NameMetadataUnreadable` observation.

No unreadable case becomes completed cleanup authority.

## What this closes

The model establishes that restart cleanup cannot infer absence from an incomplete bounded search. It provides explicit evidence for:

- exact expected identity;
- conflicting expected-name identity;
- matching identity under another name;
- complete absence;
- bounded/truncated uncertainty;
- unreadable expected-name uncertainty.

## What remains open

The entries are still supplied to the model. The next experiment must derive them from the actual descriptor-pinned Linux staging directory, with no-follow child opens and charged metadata inspection.

Real AEAD/private-stage confidentiality, durable authenticated journal storage/anti-rollback, the same-UID final check-to-unlink race, and physical filesystem qualification remain open.

## Verification

Implementation head `5e2e39d58abb659f8393ef0d3bc72065b6b0fd67` is green on the decisive Experiment 0164 gates in Rust workflow run `31786001765`:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all ten bounded restart-inventory regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The workflow continued into broader HTTP/source and repository evidence replay after the implementation gate passed.

## Next executable slice

Experiment 0165 should scan the already-open descriptor-pinned Linux staging directory itself and feed the resulting bounded facts through this classifier. Every child identity lookup must be no-follow and charged. Non-UTF8, disappearing, symlink, or otherwise unreadable children must prevent false absence proof rather than being skipped.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
