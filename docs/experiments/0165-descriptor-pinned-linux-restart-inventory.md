# Experiment 0165 — descriptor-pinned Linux restart identity inventory

**Status:** non-normative Phase 3 Linux restart evidence; persistent artifact identity remains journal-derived research state  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0162–0164

## Purpose

Experiment 0164 defines a bounded restart identity classifier but receives entry facts from the caller. Experiment 0165 derives those facts from the real descriptor-pinned Linux staging directory used by the current private-publication research backend.

The scanner:

- enumerates through the already-open staging directory authority;
- never re-resolves the original staging pathname;
- opens children with the existing no-follow relative-open helper;
- charges name/metadata work;
- bounds directory entries and metadata bytes;
- separately bounds content bytes used to confirm an identity;
- streams observations directly into the exact Experiment 0164 classifier without collecting the directory first.

## Identity strengthening discovered during execution

The first scanner draft treated hashed `(dev, ino)` as restart artifact identity.

Real Linux execution exposed inode reuse twice: after unlinking the expected staged file, a subsequently created unrelated file inherited the same inode. The classifier therefore observed a false matching identity elsewhere.

That behavior was fail-closed in the observed cases, but the same reuse at the expected name could be unsafe if `(dev, ino)` alone were allowed to authorize `RetryExactCleanup`.

The experiment was therefore strengthened before being accepted.

`(dev, ino)` is now only a **candidate hint**. Exact identity requires all of:

- device matches;
- inode matches;
- exact staged length matches;
- SHA-256 of the candidate contents matches the expected staged SHA-256.

The confirmed identity token domain-separates and hashes those values. A same-inode candidate with changed content is classified as a different identity.

This is the first important result of Experiment 0165: allocator metadata is not sufficient persistent restart authority.

## Content-verification budget

Content confirmation has its own `max_identity_bytes` budget.

The scanner hashes a candidate only when its `(dev, ino, len)` matches the expected hint. Before hashing, the candidate length must fit the remaining content-verification budget.

If the budget is insufficient, that candidate is emitted to the Experiment 0164 classifier as unreadable. For the expected name this becomes explicit `NameMetadataUnreadable`; it never becomes exact identity.

Therefore resource pressure cannot downgrade content confirmation into metadata-only trust.

## Pinned directory authority

The scanner first validates the `/proc/self/fd/<dirfd>` directory binding and enumerates that already-open directory.

A regression renames the original staging directory and creates a replacement directory at the old pathname containing replacement bytes under the expected staged name.

The scanner still observes the original pinned directory and exact original staged artifact. The replacement pathname cannot redirect restart inventory.

## No-follow child inspection

Every child is opened through the existing descriptor-relative, `O_NOFOLLOW` helper from the Linux staging backend.

A symlink child is treated as unreadable rather than followed. Because Experiment 0164 forbids absence proof in the presence of unreadable entries, a symlink cannot be silently skipped on the way to `MissingNoMatchingIdentityCompleteScan`.

## Real filesystem cases

The executable Linux regressions cover:

- expected name with exact confirmed identity;
- same inode and length but changed contents -> different identity;
- insufficient content-verification budget -> expected-name metadata/identity indeterminate;
- expected artifact renamed -> matching confirmed identity elsewhere;
- complete scan after unlink -> absence only when no matching confirmed identity survives;
- moved original plus replacement at expected name -> different identity;
- original staging pathname replaced -> scanner remains pinned to original directory;
- entry-count truncation -> cannot prove absence;
- metadata-byte truncation -> cannot prove absence;
- no-follow symlink child -> unreadable/truncated rather than false absence.

## Streaming and bounds

`fs::read_dir` is mapped lazily into the exact Experiment 0164 tuple adapter. The directory is not collected before classification.

Experiment 0164 retains authority over:

- maximum scanned entries;
- maximum charged metadata bytes;
- unreadable-entry behavior;
- positive matching-identity evidence;
- complete-absence proof.

Experiment 0165 adds only filesystem derivation plus bounded content confirmation.

## Important identity boundary

The SHA-256-confirmed identity is suitable research evidence for restart classification only when the expected digest/length comes from authenticated journal authority.

The Linux filesystem itself does not provide that trust anchor. Device/inode values may be reused, and a digest recomputed solely from whatever is currently present would not identify the original artifact.

Production therefore needs the authenticated cleanup journal to persist the expected content digest and length created before cleanup begins. The restart scanner may then confirm current filesystem candidates against that authenticated expected value.

## What this closes

Experiment 0165 removes the modeled-entry gap from Experiment 0164 for Linux research execution:

- restart inventory is derived from the already-open descriptor-pinned staging directory;
- original pathname replacement cannot redirect it;
- child inspection is no-follow;
- directory and metadata work are bounded;
- content confirmation is separately bounded;
- inode reuse alone cannot authorize an exact retry;
- changed content under the same inode is rejected;
- insufficient identity-verification budget remains indeterminate;
- only a complete readable scan can prove absence.

## What remains open

The final restart bridge still needs to feed these concrete `InventoryObservation` values directly into Experiment 0163's crash-authoritative restart disposition.

Production issue #11 also still requires:

- real vetted AEAD/private-stage confidentiality;
- a durable authenticated journal that persists expected artifact digest/length and cleanup generations;
- anti-rollback authority;
- crash-consistent execution of prepared/terminal journal transitions around the descriptor-pinned unlink;
- resolution of the same-UID final identity-check -> unlink race or an explicit isolation assumption;
- physical power-loss/filesystem qualification;
- qualified non-Linux behavior or an explicit Linux-only production scope.

## Verification

Implementation head `44de70424011a69870f10118c125fe0a59d21801` is green on:

- workspace formatting;
- Clippy with warnings denied;
- targeted local Linux restart-inventory tests;
- repository Rust implementation tests including the strengthened Linux restart-inventory regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The repository Rust workflow for this head completed successfully.

## Next executable slice

Experiment 0166 should close the last logical restart seam by feeding the exact Experiment 0165/0164 observation into the Experiment 0163 disposition function:

- `ExactIdentity` -> `RetryExactCleanup`;
- `DifferentIdentity` -> `RetainIndeterminate`;
- `MissingNoMatchingIdentityCompleteScan` -> `SyncDirectoryThenFinalize`;
- `MissingMatchingIdentityElsewhere` -> `ResolveRenamedPrivate`;
- `MissingScanTruncated` or `NameMetadataUnreadable` -> `RetainIndeterminate`.

The end-to-end Linux tests should derive the observation from the real pinned directory, then require the crash-authoritative disposition without a caller-supplied intermediate classification.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
