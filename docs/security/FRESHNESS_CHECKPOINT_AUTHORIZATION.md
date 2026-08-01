# Freshness checkpoint authorization guidance

**Status:** Phase 3 security guidance; not an epoch allocation or application policy mandate

## 1. Purpose

Authenticated bytes establish integrity relative to one snapshot identity. They do not establish that the snapshot is the newest authorized state. Applications that require rollback or fork detection need a separately protected trusted checkpoint and explicit rules for changing it.

## 2. Checkpoint contents

A trusted checkpoint should bind at least:

- the file or logical object namespace;
- accepted sequence number;
- accepted snapshot digest;
- accepted commit digest;
- the format/profile identifier used to interpret the file;
- the authorization principal or policy version that approved the checkpoint;
- a monotonic local revision or transaction identifier for the checkpoint store.

The namespace binding must prevent a valid checkpoint for one file, tenant, repository, or account from authorizing another.

## 3. Comparison outcomes

Applications must keep these outcomes distinct:

- **Unpinned integrity:** the file is internally valid, but no trusted freshness checkpoint exists;
- **Current:** sequence and identities equal the trusted checkpoint;
- **Authorized advance candidate:** sequence is greater and ancestry or application policy permits consideration;
- **Rollback:** sequence is lower than the trusted checkpoint;
- **Same-sequence fork:** sequence is equal but snapshot or commit identity differs;
- **Unrelated or unverifiable advance:** sequence is greater but required ancestry, authority, or profile evidence is absent.

Only `Current` is automatically accepted as already authorized. Every other outcome requires an explicit application decision.

## 4. Initial pin authorization

The first checkpoint must not be silently created merely because a file validates. Initial pinning requires one of:

- a user or administrator action in an authenticated session;
- a signed provisioning record whose verification key is already trusted;
- a protected deployment policy naming the expected identity or trusted origin;
- a quorum or transparency-log rule defined by the application profile.

The application must surface that initial pinning establishes a new trust anchor rather than confirming prior freshness.

## 5. Advance authorization

A greater sequence must not automatically overwrite the checkpoint solely because it is numerically newer. An advance is authorized only when application policy validates the required combination of:

- authenticated parent linkage or an explicitly allowed reissuance boundary;
- writer, signer, repository, or service authority;
- namespace and profile continuity;
- required user, administrator, quorum, or policy approval;
- absence of an unresolved same-sequence fork;
- any application-specific retention, audit, or change-control requirements.

Approval should bind the exact candidate sequence and identities. A later candidate must be evaluated separately.

## 6. Rollback and fork handling

Rollback and same-sequence fork outcomes are security events. Applications must:

- reject automatic use for freshness-sensitive operations;
- preserve the trusted checkpoint unchanged;
- retain the candidate identities and relevant source/version evidence for diagnosis;
- avoid retry logic that converts the event into a generic transport failure;
- require a separately authenticated recovery or trust-reset procedure before accepting the candidate.

A trust reset must be visibly distinct from an ordinary advance and should preserve an audit record of the prior checkpoint.

## 7. Checkpoint-store durability

A checkpoint store used for security decisions must provide:

- atomic compare-and-swap or transaction semantics over the complete checkpoint;
- crash-consistent persistence before reporting an accepted advance;
- protection against unprivileged modification and namespace substitution;
- monotonic update detection where the platform provides it;
- explicit reporting of indeterminate persistence outcomes;
- backup and recovery rules that do not silently restore an older checkpoint;
- concurrency control preventing two candidates from both being reported as the accepted successor.

If durable persistence is indeterminate, the application must not report the candidate as durably accepted. It must re-read and reconcile the checkpoint store before continuing.

## 8. Offline and replicated applications

Offline devices and replicated clients may observe legitimate divergent candidates. Profiles must specify whether they use:

- one authoritative writer;
- signed append ancestry;
- quorum approval;
- a transparency or consistency log;
- explicit user conflict resolution;
- a merge/reissuance protocol creating a new authorized lineage.

Sequence comparison alone is insufficient to resolve concurrent or partitioned writers.

## 9. User-interface requirements

User-facing tools should distinguish:

- “integrity verified” from “freshness verified”;
- “new trust anchor” from “authorized advance”;
- “rollback detected” from “file corrupt”;
- “same-sequence fork detected” from “newer version available”;
- “checkpoint persistence uncertain” from “advance completed.”

Diagnostic or salvage commands must never update the trusted checkpoint implicitly.

## 10. Evidence and testing

Applications claiming freshness protection should test:

- first pin with and without authority;
- exact current identity;
- valid authorized advance;
- greater sequence lacking authority;
- lower-sequence rollback;
- same-sequence fork;
- concurrent advance races;
- crash before and after checkpoint-store commit;
- indeterminate store outcome and reconciliation;
- backup restoration attempting to roll back the checkpoint;
- explicit trust reset with audit preservation.

This guidance defines the application decision boundary. The UCOF byte format can provide authenticated sequence and ancestry evidence, but cannot decide who is authorized to establish or change local trust.
