# Experiment 0160 — generation-bound stale cleanup execution authorization

**Status:** non-normative Phase 3 authorization evidence; **destructive effect is simulated, not filesystem mutation**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0158–0159

## Purpose

Experiment 0158 bounds stale cleanup planning and keeps publication-indeterminate state categorically non-destructive. Experiment 0159 binds nonce activation to an authenticated journal generation. A remaining planner-to-executor gap is time-of-check/time-of-use drift: journal authority or artifact identity may change after an action is planned but before a destructive effect occurs.

This experiment inserts an authenticated execution-authorization boundary between planning and mutation.

The model performs only a simulated destructive effect by changing an in-memory artifact from present to absent. No filesystem deletion occurs.

## Authorization claims

A cleanup authorization binds all of:

- operation identity;
- authenticated journal generation;
- authenticated cleanup authority;
- exact destructive action kind;
- expected artifact identity;
- expected private-byte charge.

The authorization is sealed independently from the journal.

The test authenticator uses SHA-256 domain-separated tags as plumbing only. It is **not a production MAC/AEAD, is not a cryptographic security claim, and provides no confidentiality**.

## Allowed authority/action mapping

Only the exact destructive mappings are accepted:

- `ResumeOrDiscardPrivate` -> `DiscardPrivate`;
- `CleanupDurablePrivate` -> `CleanupDurablePrivate`;
- `TerminalDiscarded` -> `CleanupDiscardedRemnants`.

`ResolvePublication` has no destructive mapping.

A validly sealed token containing a different action is rejected during execution even if every identity/generation field otherwise matches.

## Planning authorization

`plan_authorization` first authenticates the journal state.

It then requires:

1. authority is not `ResolvePublication`;
2. the requested action is valid for that authority;
3. the expected artifact is currently present.

Only then is a sealed authorization created from the exact journal generation and artifact identity/byte count.

Therefore publication-indeterminate authority cannot even produce a destructive token in this model.

## Execution revalidation

Immediately before the simulated destructive effect, `execute_authorized_cleanup` revalidates both sides independently:

1. authenticate the **current** journal;
2. authenticate the authorization token;
3. reject current `ResolvePublication` authority;
4. require current operation, generation, and authority to equal the token claims;
5. require the claimed action to remain valid for the current authority;
6. require the artifact still to exist;
7. require current artifact identity and byte charge to equal the token claims.

Only after all checks pass does the model mark the artifact absent.

## Exact successful cases

A regression executes each allowed authority/action pair once and requires the artifact to become absent only after successful revalidation.

No other pair is accepted.

## Journal generation TOCTOU

A regression plans `DiscardPrivate` at generation 7, advances the authenticated journal to generation 8 before execution, and requires:

- `JournalChanged`;
- artifact remains present.

A token therefore cannot authorize a later journal generation merely because operation identity and authority are unchanged.

## Authority TOCTOU

A regression plans under `ResumeOrDiscardPrivate` and then changes the authenticated journal authority to `ResolvePublication` before execution.

Execution returns `ResolvePublication` and leaves the artifact present.

A second regression changes to a different destructive authority and requires `JournalChanged` with no effect.

Therefore authority transitions invalidate old destructive authorization rather than inheriting it.

## Operation substitution

A valid token cannot be replayed against another operation with the same generation and authority.

Changing only operation identity produces `JournalChanged`; the artifact remains present.

## Artifact TOCTOU

After authorization planning, regressions independently change:

- artifact identity;
- private-byte charge.

Both produce `ArtifactChanged` and leave the artifact present.

This creates an explicit seam for a future descriptor/inode identity recheck immediately before real removal.

## Authentication failure

Regressions modify:

- authenticated journal generation without recomputing its tag;
- token private-byte claims without recomputing its tag.

Both fail as `AuthenticationFailed` before any destructive effect.

## Unauthorized-action substitution

A correctly sealed authorization is constructed with matching operation/generation/authority/artifact fields but an action not permitted by the current authority.

Execution rejects it as `UnauthorizedAction` and leaves the artifact present.

Authentication therefore does not replace semantic authorization.

## Replay after cleanup

After one successful simulated cleanup, replaying the same token against the now-absent artifact returns `ArtifactMissing`.

The same authorization cannot produce a second destructive effect in the model.

## What this closes

The model establishes executable evidence that a stale cleanup action can be separated from mutation by a generation-bound authorization whose execution fails closed on:

- journal-generation drift;
- authority drift, especially transition to `ResolvePublication`;
- foreign operation substitution;
- artifact identity replacement;
- artifact byte/accounting change;
- journal authentication failure;
- authorization authentication failure;
- unauthorized action substitution;
- replay after the artifact is already absent.

All failure regressions require zero simulated destructive effect.

## What remains open

This experiment deliberately stops before filesystem mutation. Production work still requires:

- a vetted MAC/AEAD or equivalent journal/authorization authentication primitive;
- durable authenticated journal storage and anti-rollback authority;
- deriving authorization directly from the bounded Experiment 0158 planner rather than a separate test call;
- descriptor-pinned or equivalent hardened artifact handles;
- exact filesystem identity binding, including device/inode or a stronger platform handle;
- cleanup-result journaling and restart authority;
- physical power-loss/filesystem qualification.

The current Linux descriptor-pinned backend in PR #130 is a useful next integration target: it pins staging/destination directories, refuses pathname redirection after begin, and rechecks staged `(dev, ino)` identity before cleanup. Its documented identity-check -> `remove_file` sequence remains two separate operations, so it does **not** close a sufficiently privileged same-UID final-step name race. A future integration must preserve that boundary rather than describe the delete as atomic.

## Verification

Implementation head `bc981f4a3ac9d0b394bd11bb51263866cb993e33` is green on the decisive Experiment 0160 gates in Rust workflow run `31783274723`:

- locked dependency graph;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all nine cleanup-authorization and TOCTOU regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The workflow continued into the repository's broader HTTP/source and evidence replay after the implementation gate passed. Those checks provide additional validation breadth but are not required to establish the authorization result.

## Next executable slice

Experiment 0161 should join this authorization boundary to a hardened **pinned-artifact execution interface** without yet claiming atomic hostile-filesystem deletion.

A useful test interface should require the executor to expose an observed artifact identity from an already-pinned/open object and then revalidate that identity at the last available point before mutation. The authorization token must bind that identity. Tests should cover:

- original directory pathname replacement cannot redirect execution;
- observed staged-name replacement rejects before mutation;
- journal generation/authority changes still invalidate execution;
- pinned artifact identity mismatch rejects execution;
- destination/publication-indeterminate state remains non-destructive;
- executor failure reports no successful cleanup authority.

The same-UID check-to-unlink race remains an explicit unresolved platform primitive gap until an atomic handle-relative removal mechanism or stronger isolation assumption is selected.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
