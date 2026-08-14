# Experiment 0155 — private-stage AEAD integration contract

**Status:** non-normative Phase 3 contract evidence; **no production confidentiality claim**  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiment 0154 bounded private publication

## Purpose

Experiment 0154 closes the bounded-memory/private-publication composition gap, but all research spill and retained-stage bytes are still plaintext. Before introducing a production cryptographic dependency, this experiment makes the private-record cryptographic contract executable and adversarially testable.

The experiment deliberately does **not** implement production encryption. It defines the exact identity, nonce, associated-data, ordering, and authenticated-resume invariants that a vetted AEAD adapter must satisfy.

The test adapter is intentionally non-confidential: it leaves plaintext visible and appends a deterministic SHA-256-based test authenticator only to detect incorrect nonce/AAD plumbing. Therefore a green result from this experiment must not be described as encrypted spill or encrypted staging.

## Why the AEAD dependency is not wired in this slice

`ucof-experiments` currently has no direct AEAD dependency. The workspace is tested with `cargo metadata --locked`, and the connector used for this research does not provide a narrow Cargo lockfile-edit workflow.

Although cryptographic libraries can exist transitively in the lockfile, relying on a transitive package as if it were a direct crate dependency would be incorrect Cargo dependency hygiene and could invalidate the locked dependency gate.

The experiment therefore keeps the cryptographic backend behind a trait until a vetted AEAD can be added through a normal Cargo dependency/lockfile update and reviewed as an explicit dependency change.

## Private record identity

Every private record is authenticated against an exact identity tuple:

- 16-byte operation ID;
- 16-byte key ID;
- stage kind;
- segment ID;
- record sequence number;
- declared plaintext length.

The modeled stage kinds are:

- sorter run;
- sorted source descriptor stage;
- locator stage;
- page-reference stage.

The private-record identity has no UCOF wire compatibility meaning.

## Associated-data layout

The model uses a 72-byte canonical associated-data prefix containing:

- 8-byte private-record magic `UCOFSTG1`;
- version byte;
- stage-kind byte;
- six reserved zero bytes;
- 16-byte operation ID;
- 16-byte key ID;
- segment ID (`u64` little-endian);
- sequence (`u64` little-endian);
- declared plaintext length (`u64` little-endian).

The outer private-record header additionally carries the 12-byte nonce and 12 reserved zero bytes.

Reserved bytes are required to remain zero so future private framing changes cannot be silently accepted as the current contract.

## Nonce allocation contract

One `OperationCryptoContext` owns nonce allocation for the operation.

A nonce is 12 bytes:

- 4-byte per-operation prefix;
- 8-byte big-endian monotonic counter.

The same counter namespace is intended to be shared across every private record class used by the operation. Stage kinds or segment IDs are **not** separate nonce namespaces.

This avoids the common failure mode where two subsystems each begin a local counter at zero under the same key/prefix.

## Exhaustion behavior

The counter uses checked increment.

The final valid nonce contains `u64::MAX`. After it is issued, the allocator records an exhausted state. The next allocation fails with `NonceExhausted`; the counter never wraps to zero.

## Authenticated resume boundary

The restart checkpoint contains:

- operation ID;
- key ID;
- nonce prefix;
- next nonce counter, including exhausted state.

The model refuses to resume from a checkpoint unless the caller states that the checkpoint has already been authenticated.

This is a contract boundary, not yet an authenticated journal implementation. Experiment 0156 should make the authentication and rollback rules concrete at the journal/state-machine layer.

## Identity/AAD substitution evidence

The round-trip regression verifies that changing any expected record identity causes authentication failure, including:

- operation ID;
- key ID;
- stage kind;
- segment ID.

The sequence is exercised independently in the reorder tests. Declared length is authenticated as AAD and is also checked against the recovered plaintext length.

## Corruption, truncation, and reorder evidence

The adversarial tests require failure for:

- payload corruption;
- test-authenticator corruption;
- declared-length substitution;
- non-zero reserved bytes;
- body truncation;
- header truncation;
- presenting record sequence 1 where sequence 0 is expected;
- presenting record sequence 0 where sequence 1 is expected.

The record opener requires the exact expected identity supplied by the stage reader; records do not self-authorize their own segment/sequence position.

## Million-nonce campaign

A constant-memory regression allocates **1,000,000** nonces from one operation context.

For every counter value it requires:

- the four-byte prefix to remain unchanged;
- the trailing eight bytes to decode exactly to the expected big-endian counter;
- the final checkpoint to contain `next_counter = 1,000,000`.

The test does not retain a million-element set. Uniqueness follows directly from the checked injective prefix+counter construction while still exercising every generated nonce value.

## Exhaustion campaign

A dedicated boundary regression begins at `u64::MAX - 1` and requires:

1. nonce counter `u64::MAX - 1`;
2. nonce counter `u64::MAX`;
3. `NonceExhausted` on the next request.

No wraparound is accepted.

## Resume campaign

A context allocates counters 0, 1, and 2, then exports a checkpoint.

The regression requires:

- unauthenticated resume to fail;
- authenticated resume to succeed;
- the first resumed nonce to use counter 3.

This verifies counter continuity across the modeled restart seam.

## Private randomness cannot affect recovered canonical input

Two records for identical private plaintext and record identity are sealed under different nonce prefixes.

The private record bytes differ, but both open to the exact same plaintext.

This is the required architectural direction for the production writer: private staging may be randomized/encrypted while deterministic final canonical UCOF bytes remain a function of canonical plaintext state rather than spill ciphertext.

## Explicit non-confidential test adapter

The test adapter copies plaintext directly into the private record body and appends a deterministic digest-based test authenticator.

A regression explicitly asserts that the plaintext is visible in the staged record.

That assertion is intentional. It prevents this experiment from being accidentally cited as evidence that spill confidentiality already exists.

The digest suffix is test plumbing, not a production MAC construction and not a substitute for a vetted AEAD.

## Verification

Implementation head `e756cae8a980cda39f54d6fa86d7ad5b5bd06499` is green on the decisive implementation gates:

- locked dependency graph check;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0155 nonce/AAD/corruption/reorder/resume cases and the prior bounded-publication regressions inherited from the branch;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The repository's longer protocol/policy/parser/vector/framing replay continues after the implementation gate and provides broader regression confidence rather than changing the cryptographic-contract result.

## What this closes

This experiment closes the ambiguity around the **shape** of encrypted/authenticated private records:

- one operation-wide nonce namespace;
- no counter wrap;
- authenticated identity and stage position;
- authenticated declared length;
- fail-closed corruption/truncation/reorder behavior;
- authenticated checkpoint requirement for resume;
- private randomness independent of recovered canonical plaintext.

## What remains open

This experiment does **not** close the encrypted-spill requirement in issue #11.

Still required:

1. add and review a vetted AEAD dependency through a normal Cargo/lockfile workflow;
2. implement real confidentiality and authentication with that AEAD;
3. define operation-key generation/derivation and key-ID provenance;
4. define nonce-prefix generation and prove uniqueness across operations using the same key;
5. authenticate and durably publish the restart journal before any checkpoint can authorize resume;
6. prove rollback resistance for journal generations and nonce counters;
7. encrypt/authenticate sorter runs and every retained descriptor/locator/page-reference stage;
8. include AEAD framing/tag overhead in the operation-wide private-storage quota;
9. define crash behavior around journal sync versus nonce issuance so restart can never reuse a nonce;
10. re-run deterministic canonical-output equivalence with randomized real ciphertext.

## Next executable slice

Experiment 0156 should model the authenticated restart/discard journal and authority state machine. It should bind operation/key identity, journal generation, next nonce counter, private-artifact inventory, publication state, and restart/discard authority; reject foreign-operation substitution, generation rollback, nonce rollback, truncation, and unauthenticated checkpoints; and preserve indeterminate publication states without destructive cleanup.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, change any proposed immutable-successor bytes, allocate a UCOF epoch, or create a compatibility promise.