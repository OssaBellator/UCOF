# Experiment 0166 — Linux crash-authoritative cleanup restart bridge

**Status:** non-normative Phase 3 Linux restart-authority evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0163–0165

## Purpose

Experiments 0163–0165 separately establish crash-authoritative cleanup ordering, bounded restart identity classification, and real descriptor-pinned Linux filesystem observation. A final trust seam remained if callers could inspect an intermediate inventory observation and reinterpret it before restart authority was selected.

Experiment 0166 closes that seam with one callable bridge from the concrete bounded inventory observation to prepared-cleanup restart disposition.

The Linux end-to-end path now performs:

`pinned directory scan -> bounded no-follow identity classification -> content-confirmed observation -> cleanup restart disposition`

The caller receives the disposition, not a policy-free observation to reinterpret.

## Canonical prepared-cleanup mapping

The bridge maps every Experiment 0164 observation exactly once:

- `ExactIdentity` -> `RetryExactCleanup`;
- `DifferentIdentity` -> `RetainIndeterminate`;
- `MissingNoMatchingIdentityCompleteScan` -> `SyncDirectoryThenFinalize`;
- `MissingMatchingIdentityElsewhere` -> `ResolveRenamedPrivate`;
- `MissingScanTruncated` -> `RetainIndeterminate`;
- `NameMetadataUnreadable` -> `RetainIndeterminate`.

The bridge has an exhaustive regression covering all six cases.

## Linux end-to-end cases

The descriptor-pinned Linux integration calls the Experiment 0165 scanner and immediately applies the canonical bridge.

Executable cases prove:

- an exact, SHA-256-confirmed expected artifact becomes `RetryExactCleanup`;
- the confirmed artifact renamed elsewhere becomes `ResolveRenamedPrivate`;
- complete readable absence becomes `SyncDirectoryThenFinalize`;
- expected-name replacement becomes `RetainIndeterminate`;
- bounded/truncated inventory becomes `RetainIndeterminate`;
- no-follow symlink/unreadable inventory becomes `RetainIndeterminate`.

No end-to-end test exposes an intermediate observation to a caller-selected action.

## Identity boundary inherited from Experiment 0165

Restart exactness still requires the authenticated expected artifact identity to include:

- device/inode as a candidate hint;
- exact length;
- expected SHA-256 content digest.

Content confirmation is bounded separately. Inode reuse alone cannot authorize `RetryExactCleanup`.

A content-verification budget failure remains unreadable/indeterminate.

## Crash-authority interpretation

This bridge is specifically for a durable `CleanupPrepared` restart state.

The resulting dispositions align with Experiment 0163's crash ordering:

- exact artifact -> retry the exact prepared cleanup;
- complete proven absence -> synchronize the staging directory and finish terminal journal finalization;
- renamed confirmed artifact -> explicit renamed-private resolution;
- conflicting, truncated, or unreadable state -> retain indeterminate authority.

`ResolvePublication` remains outside this destructive prepared-cleanup path and is still non-destructive under Experiments 0160–0163.

## What this closes

Experiment 0166 removes the final logical restart-classification seam in the current Linux research sequence:

- the filesystem scanner derives facts from pinned authority;
- the exact bounded classifier decides whether absence is proven;
- SHA-256 confirmation prevents metadata reuse from becoming exact identity;
- one exhaustive bridge selects the prepared-cleanup restart disposition;
- callers no longer choose how to reinterpret inventory observations.

## What remains open

The remaining issue #11 gates are now implementation/platform assurance rather than missing logical policy:

- a real vetted AEAD/private-stage confidentiality implementation;
- a real durable authenticated journal persisting expected digest/length and cleanup generations;
- anti-rollback authority;
- crash-consistent execution of the prepared/terminal journal transitions around the real descriptor-pinned unlink;
- resolution of the same-UID final identity-check -> unlink race or an explicit stronger isolation assumption;
- physical power-loss/filesystem qualification;
- qualified non-Linux behavior or explicit Linux-only production scope.

## Verification

The accepted Experiment 0166 branch head is green on:

- workspace formatting;
- Clippy with warnings denied;
- targeted local Linux restart-bridge tests;
- repository Rust implementation tests including the Linux scanner and restart bridge regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The repository Rust workflow for the accepted branch head completed successfully.

## Next executable slice

The highest-value next slice is a real AEAD implementation for private-stage records. The current repository already has the logical nonce/AAD/journal contracts, and `aws-lc-rs 1.18.0` is present transitively in the lock graph. A safe implementation must add the dependency without weakening `--locked`, preserve the Experiment 0157 nonce-lease rule, and prove real ciphertext confidentiality/integrity behavior.

If dependency/lock generation remains blocked, the alternative next platform slice is to integrate Experiment 0163's prepared/terminal journal ordering with the actual descriptor-pinned cleanup executor and inject failures around unlink, directory sync, and terminal journal commit.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
