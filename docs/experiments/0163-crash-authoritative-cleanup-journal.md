# Experiment 0163 — crash-authoritative cleanup journal

**Status:** non-normative Phase 3 crash-ordering evidence; durable commit remains a modeled backend boundary  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiment 0162

## Purpose

Experiment 0162 proves authorized descriptor-pinned Linux cleanup but does not make cleanup state authoritative across a crash. This experiment defines the minimum journal ordering needed to avoid both premature deletion and premature terminal authority.

The required order is:

`cleanup-prepared journal durable -> unlink -> staging-directory sync -> terminal-cleanup journal durable`

No later step can become authoritative before the earlier durability boundary is satisfied.

## Journal phases

The model carries:

- operation identity;
- journal generation;
- cleanup authority;
- phase.

Phases are:

- `PrivateActive`;
- `CleanupPrepared(ArtifactBinding)`;
- `TerminalDiscarded(ArtifactBinding)`.

`ArtifactBinding` contains the exact artifact identity and private-byte charge.

## Prepared generation

A cleanup-prepared generation may be planned only from `PrivateActive` and never from `ResolvePublication` authority.

The candidate increments journal generation and binds the exact artifact.

A pending prepared generation cannot start destructive execution until the backend reports it durably committed. A crash before that point restarts as ordinary private state.

## Unlink and directory sync

After prepared durability, execution may mark unlink complete only for the exact bound artifact.

Staging-directory sync cannot be marked before unlink completion.

A terminal cleanup generation cannot even be planned until both are true:

1. unlink completed for the exact artifact;
2. the staging directory was synchronized.

This prevents an in-memory unlink result from becoming durable cleanup authority before namespace durability is established.

## Terminal generation

The terminal candidate increments generation again and preserves the exact artifact binding.

It becomes authoritative only after the terminal generation itself is durably committed.

Therefore terminal authority is downstream of both namespace removal and staging-directory durability.

## Crash cuts

The executable regressions cover all major cuts.

### Before prepared durability

Cleanup cannot activate. Restart disposition is `ResumePrivate`.

### After prepared durability, before unlink

The durable journal is `CleanupPrepared`. If the expected name resolves to the exact artifact identity, restart disposition is `RetryExactCleanup`.

### After unlink, before directory sync

The journal remains `CleanupPrepared`; no terminal generation can be planned. A complete restart inventory that finds the expected name absent and no matching identity elsewhere yields `SyncDirectoryThenFinalize`.

### After directory sync, before terminal durability

The terminal candidate may be planned, but failure to durably commit it leaves the durable journal in `CleanupPrepared`. Restart can finish the namespace durability/finalization path without treating the unpublished terminal candidate as authority.

### After terminal durability

`TerminalDiscarded` is authoritative after restart regardless of incidental current pathname observation.

## Rename ambiguity

A missing expected name is not treated as completed cleanup when the same artifact identity is found under another name.

That observation yields `ResolveRenamedPrivate`, preserving Experiment 0162's rule that rename must not be mistaken for deletion.

If inventory is truncated, or the expected name resolves to a conflicting identity, restart yields `RetainIndeterminate` rather than finalizing cleanup.

## Safety failures

The model also requires:

- `ResolvePublication` cannot enter `CleanupPrepared`;
- journal generation drift makes a prepared reservation stale;
- artifact mismatch prevents marking unlink complete;
- generation overflow fails before a new cleanup phase is created.

## What this closes

Experiment 0163 supplies executable crash-ordering evidence for the cleanup side of issue #11:

- destructive cleanup cannot start before durable prepared authority;
- terminal cleanup cannot be planned before unlink plus staging-directory sync;
- terminal cleanup cannot become authoritative before its own durable journal commit;
- restart distinguishes exact retry, complete absence, renamed matching identity, conflicting identity, and truncated inventory;
- missing-name evidence alone is never sufficient when identity inventory is incomplete;
- `ResolvePublication` remains categorically outside destructive cleanup.

## What remains open

This is still a logical durability model. Remaining production work includes:

- a real authenticated durable journal backend implementing these generation transitions;
- a bounded filesystem identity inventory that derives restart observations rather than receiving them as caller assertions;
- integration of the prepared/terminal journal transitions with the descriptor-pinned Linux executor;
- real vetted AEAD/private-stage confidentiality;
- anti-rollback authority;
- resolution of the documented same-UID final check-to-unlink race or an explicit isolation assumption;
- physical power-loss/filesystem qualification.

## Verification

Implementation head `999e07b9e75cba77348d45f0a5fe189d32fa1c71` is green on the decisive Experiment 0163 gates in Rust workflow run `31785395271`:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all eleven cleanup crash-cut/state-machine regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The workflow continued into the repository's broader HTTP/source and evidence replay after the implementation gate passed.

## Next executable slice

The next slice should derive the restart observation from a **bounded directory identity inventory** instead of accepting it as an input.

The inventory must cap directory entries and metadata bytes, recognize the exact expected staged identity, detect the same identity under another name, distinguish a conflicting expected-name identity, and return an explicit truncated/indeterminate result whenever the scan bound prevents proving absence.

Only a complete bounded scan with no matching identity may feed `SyncDirectoryThenFinalize`.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
