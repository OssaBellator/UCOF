# Experiment 0162 — authorized descriptor-pinned Linux cleanup

**Status:** non-normative Phase 3 Linux execution evidence; **same-UID final check-to-unlink race remains open**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0160–0161 and the descriptor-pinned backend from Experiment 0147 / PR #130

## Purpose

Experiments 0160–0161 establish generation/artifact-bound cleanup authorization but stop before filesystem mutation. PR #130 establishes a Linux descriptor-pinned staging backend that prevents later replacement of the original staging/destination directory pathnames from redirecting publication or cleanup, and rechecks staged `(dev, ino)` identity before unlink.

This experiment joins those two lines of evidence for the first time.

The exact PR #130 backend source is imported unchanged into the current writer lineage, then an authorization layer inside the backend's defining module binds cleanup to:

- operation identity;
- journal generation;
- cleanup authority;
- the already-open staged inode identity;
- staged byte length.

Execution revalidates those claims immediately before calling the descriptor-pinned backend cleanup path.

## Backend provenance

The Linux backend is not reimplemented from memory. The exact existing repository blobs from `phase-3/linux-descriptor-staging` are reused in this branch and wired into the evolved `immutable_successor` module.

The current writer therefore retains all newer bounded-source/tree/publication work while reusing the previously validated Linux descriptor-pinning implementation.

## Pinned artifact identity

The cleanup authorization derives its artifact identity from the already-open staged file, not from the original staging pathname.

The research identity hashes:

- filesystem device;
- inode number.

The private-byte charge is bound separately to the current open-file length.

This gives the authorization layer a stable identity across directory pathname replacement and ordinary name movement while the file descriptor remains open.

## Authorization

The test-only token binds:

- operation ID;
- journal generation;
- authority;
- pinned artifact identity;
- private bytes.

`ResolvePublication` cannot produce or execute destructive cleanup.

The test authenticator remains SHA-256 domain-separated plumbing only. It is not a production MAC/AEAD or confidentiality claim.

## Strict staged-name preflight

Immediately before backend cleanup, the integration performs an additional pinned-directory name preflight.

Using the already-open staging directory descriptor, it reopens the expected staged name without following symlinks and evaluates:

1. **name exists and points to the same open inode** — cleanup may proceed;
2. **name exists but points to a different inode** — reject as artifact change;
3. **name is missing and the open inode has `nlink == 0`** — the inode has no surviving directory entry and may be safely retired by dropping the handle;
4. **name is missing but the open inode still has `nlink > 0`** — classify as `StagedNameIndeterminate` and refuse cleanup success.

The fourth case closes an important ambiguity in using `NotFound` alone: a missing expected name can mean the file was renamed rather than deleted.

## Exact authorized cleanup

A regression stages real private bytes through `PersistentLinuxDescriptorStagingBackend`, derives authorization from the open inode, and executes cleanup under unchanged operation/generation/authority.

The expected private name is removed and no destructive result is reported until all authorization and pinned-identity checks pass.

## Journal change and publication-indeterminate state

A token planned at generation 7 is executed under generation 8 and must fail as `JournalChanged`, leaving the staged file present.

The same token executed after authority transitions to `ResolvePublication` must fail as `ResolvePublication`, again leaving the staged file present.

Authorization therefore does not outlive journal authority.

## Observed name replacement

A regression:

1. stages the original private file;
2. plans cleanup authorization against its open inode;
3. renames the original staged name aside;
4. creates replacement bytes at the expected staged name;
5. attempts cleanup.

The executor rejects before unlink because the expected name no longer resolves to the authorized open inode.

Both the replacement and the moved original bytes remain intact.

## Missing name with a surviving link

A separate regression renames the staged file to another name without creating a replacement.

The expected name is now missing, but the already-open original inode still has one live link.

The executor returns `StagedNameIndeterminate` and does **not** report successful cleanup. The renamed private bytes remain present for explicit recovery/reconciliation.

This is deliberately conservative: disappearance of the expected pathname is not sufficient proof that private state is gone.

## Already-unlinked inode

The converse case is also tested.

If the expected staged name is externally unlinked and the already-open file reports `nlink == 0`, the strict preflight allows retirement. Backend abort observes the missing name, drops the final open handle, and completes without a false indeterminate classification.

This distinguishes a genuinely unlinked open inode from a renamed live link.

## Directory pathname replacement

A regression begins staging, then:

1. renames the original staging directory;
2. creates a new private directory at the original path;
3. writes a marker into the replacement directory;
4. executes authorized cleanup.

Cleanup follows the already-open staging directory descriptor and removes the private name from the moved original directory.

The replacement directory and marker remain untouched.

Therefore replacing the original staging pathname after `begin_private` cannot redirect authorized cleanup.

## Foreign artifact identity and token tampering

A correctly sealed token containing a different artifact identity is rejected before backend mutation.

A token modified after sealing is rejected as authentication failure before backend mutation.

In both cases the staged private file remains present.

## What this closes

This experiment demonstrates a real Linux filesystem cleanup path with:

- journal-generation/authority-bound execution authorization;
- identity derived from an already-open staged file;
- descriptor-pinned staging-directory resolution;
- pre-unlink name-to-open-inode identity revalidation;
- original staging-path replacement immunity;
- observed staged-name replacement rejection;
- conservative handling of missing expected names with surviving hard links;
- safe retirement of already-unlinked (`nlink == 0`) open inodes;
- fail-closed behavior on generation drift, `ResolvePublication`, foreign identity, and token tampering.

## Remaining race boundary

This experiment intentionally does **not** claim atomic hostile-filesystem deletion.

The backend still performs:

1. a final staged-name identity check;
2. then a separate `remove_file` through `/proc/self/fd/<dirfd>/<name>`.

A sufficiently privileged same-UID adversary able to mutate names inside the private staging directory can theoretically race between those operations.

The additional authorization and strict preflight reduce stale/redirected cleanup risk but do not turn the check-plus-unlink pair into an atomic handle-relative removal primitive.

Production must either:

- select a stronger platform primitive/isolation mechanism that closes this race;
- or explicitly qualify and document a filesystem/ownership threat model under which same-UID concurrent mutation is excluded.

## What remains open

Issue #11 still requires:

- real vetted AEAD/private-stage confidentiality;
- durable authenticated journal and anti-rollback authority;
- crash-consistent cleanup-result journaling;
- stale-operation reconciliation after process restart;
- resolution of the same-UID final check-to-unlink race or an explicit isolation assumption;
- physical power-loss/filesystem qualification;
- cross-platform equivalents or qualified platform scope.

## Verification

Implementation head `ef2cbc9de494668d35512c0a083b37278bb06279` is green on the decisive Experiment 0162 gates in Rust workflow run `31784756701`:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all eight authorized descriptor-pinned cleanup regressions and the original PR #130 descriptor-pinning tests;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The same workflow then proceeded successfully through concrete/async HTTP source validation and the versioned S3 adapter before continuing through the repository's broader evidence replay.

## Next executable slice

The next cleanup-specific slice should make cleanup **crash-authoritative**, not just authorization-safe.

Before any unlink, an authenticated journal generation should durably enter a cleanup-prepared state bound to the exact artifact identity. After unlink, the staging directory must be synchronized before a terminal cleanup generation can be durably committed.

Crash-cut tests should distinguish at least:

- crash before cleanup-prepared journal durability — destructive execution must not have started;
- crash after prepared journal durability but before unlink — restart may retry the exact authorized cleanup;
- crash after unlink but before directory sync — cleanup durability remains unresolved;
- crash after directory sync but before terminal journal commit — restart may finish the terminal journal transition without repeating an unsafe delete;
- crash after terminal journal durability — private cleanup is authoritative.

Missing-name restart handling must retain the Experiment 0162 distinction between genuinely absent state and renamed/live-link ambiguity.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
