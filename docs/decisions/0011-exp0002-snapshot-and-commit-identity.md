# ADR-0011: Separate EXP-0002 Snapshot and Commit Identity

- **Status:** Accepted for the experimental candidate
- **Date:** 2026-07-30
- **Decision owners:** UCOF maintainers
- **Related:** ADR-0010, FCP-0002, `docs/spec/EXP_0002_BYTE_CANDIDATE.md`
- **Normative impact:** Experimental candidate only

## Context

The first concrete repair-to-new-file design exposed an identity ambiguity.

EXP-0002 candidate 1 defines:

- `snapshot_digest = SHA256(SNAPSHOT_DOMAIN || snapshot_record_bytes)`;
- `commit_digest = SHA256(COMMIT_DOMAIN || commit_bytes || footer_semantics)`.

A deterministic repair that copies every object into a new file can produce the same object offsets, directory pages, root locator, roots, and snapshot record as the source. The new file header can differ while the snapshot digest remains identical, because the file header is outside the snapshot-digest preimage. The commit digest differs because the genesis commit includes the new header.

Earlier model prose said repair must create a “new snapshot identity.” Taken literally, that would require adding an artificial nonce to canonical snapshot content or changing identity semantics merely to force inequality. That would conflate two useful identity scopes.

## Decision

For EXP-0002 candidate 1:

1. **Snapshot digest is structural snapshot identity.** It identifies the exact snapshot record, including its authenticated directory root, physical locators, root object identifiers, capability sets, sequence, parent digest, and previous-footer locator.
2. **Commit digest is file-instance commit identity.** It binds the bytes written by one commit and the footer semantics that publish that commit.
3. A repair or compaction operation must always publish a new commit and therefore a new commit digest.
4. A deterministic repair may preserve the snapshot digest only when the resulting snapshot record is byte-for-byte identical. This is not an error and must not be represented as preservation of the original file instance.
5. Byte-scoped signatures or provenance claims over the source commit do not survive repair unless a future claim format explicitly signs a stable content scope and the repaired output independently satisfies that scope.
6. APIs and reports must name the scope explicitly. Bare fields named only `identity` are not sufficient where snapshot and commit identity could be confused.

## Consequences

### Positive

- canonical output does not need a meaningless repair nonce;
- exact structural equality remains observable;
- file-instance replacement is still detected through the commit digest;
- repair reports can state precisely what was preserved and what changed;
- future signatures can choose content, snapshot, commit, or provenance-claim scope explicitly.

### Negative

- “snapshot identity” is not synonymous with “file instance identity”;
- the current snapshot digest includes physical locators, so it is not a purely logical content identity;
- callers must retain both digest scopes when comparing files;
- whole-file rollback to an older valid commit remains possible without an external freshness mechanism.

## Security analysis

An attacker cannot use equal snapshot digests alone to claim that two files are the same instance. A verifier that needs file-instance equality must compare the commit digest and, when relevant, the file identifier and external trust context.

The split does not provide authenticity. SHA-256 digests detect accidental or adversarial byte changes relative to stored digest values, but signatures, signer trust, freshness, rollback resistance, and provenance remain later phases.

Repair tools must never copy a source signature or provenance assertion while silently changing its signed scope. Until signature envelopes exist, repair reports should conservatively mark byte-scoped signatures as not preserved.

## Alternatives considered

### Add a snapshot nonce

Rejected for candidate 1. It would force distinct snapshot digests even when the authenticated snapshot structure is otherwise identical, weakening deterministic content comparison and adding another field whose security meaning is unclear.

### Include the file header in the snapshot digest

Rejected for candidate 1. It would make snapshot identity inherently file-instance-specific and reduce the ability to compare structurally identical snapshots across repaired or replicated files.

### Use only one digest identity

Rejected. One scope cannot simultaneously provide useful structural equality and file-instance commit equality without ambiguity.

## Validation requirements

The concrete repair experiment must demonstrate:

- repaired output always has a newly computed commit digest;
- identical deterministic repair may preserve the snapshot digest;
- changed object selection or physical layout changes the snapshot digest;
- source corruption prevents repair;
- reports distinguish source and output snapshot and commit digests;
- no byte-scoped signature-preservation claim is emitted.

## Revisit conditions

Revisit this decision before FCP-0002 enters Review if:

- snapshot records are redesigned to represent logical rather than physical identity;
- page reuse or relocation makes locator identity unsuitable;
- signature or provenance scope requires another canonical digest;
- independent implementations interpret either identity differently.
