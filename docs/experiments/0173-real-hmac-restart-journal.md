# Experiment 0173 — Real HMAC-SHA256 restart-journal authentication

**Status:** non-normative Phase 3 production-writer restart-authentication evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0156, 0157, 0159, 0172

## Purpose

Experiment 0173 replaces the restart journal's test-only authenticator with a real keyed MAC while preserving the existing canonical restart-journal bytes, parser, state machine, rollback fields, artifact inventory, and restart-authority logic.

The accepted native Linux x86_64 experiment uses AWS-LC RS 1.18.0 HMAC-SHA256. It authenticates journal bytes but deliberately does not encrypt them.

## Why HMAC rather than journal AEAD

The private data stages already use AES-256-GCM with crash-safe leased nonces. Reusing AEAD for the journal would introduce another nonce-durability problem around the journal itself: a process could seal a generation, crash before that generation becomes durable, then retry that same logical generation with changed bytes. A journal AEAD design would therefore need its own crash-safe nonce allocation/persistence protocol before it could safely authenticate the state that governs nonce allocation.

HMAC-SHA256 has no nonce requirement. It therefore closes keyed journal authentication without creating a circular durability dependency.

This experiment makes no journal-confidentiality claim. The canonical journal plaintext remains visible and is followed by a 32-byte HMAC tag.

## Existing canonical journal remains authoritative

0173 does not define a second journal format.

The real HMAC authenticator implements the existing `JournalAuthenticator` trait inside `private_restart_journal_contract_v2.rs`. The existing functions remain the single path for:

- canonical `encode_journal` bytes;
- exact `decode_journal` parsing;
- operation identity checks;
- key identity checks;
- minimum-generation rollback checks;
- minimum-nonce rollback checks;
- state-transition semantics;
- restart authority;
- bounded private-artifact inventory validation.

The only replacement is the authentication primitive used by `seal_journal` and `open_journal`.

## Real authentication primitive

The native experiment constructs an AWS-LC HMAC key with HMAC-SHA256.

`seal` returns:

`canonical journal plaintext || 32-byte HMAC-SHA256 tag`

`open`:

1. requires at least a complete 32-byte tag;
2. splits plaintext from tag;
3. verifies the tag with AWS-LC's HMAC verifier;
4. returns plaintext only after successful verification.

Authentication failures map to the existing `JournalError::AuthenticationFailed` result.

## Executable evidence

### Real round trip

A constructing-state journal with two private artifacts is encoded, HMAC-sealed, and reopened through the existing `open_journal` path.

The regression proves:

- sealed length is exact canonical plaintext length + 32 bytes;
- the canonical plaintext prefix is unchanged;
- operation/key/generation/nonce fields round-trip exactly;
- bounded artifact inventory round-trips exactly;
- restart authority remains `ResumeOrDiscardPrivate`.

### Field, tag, truncation, and foreign-MAC-key rejection

The authentication regression flips representative bytes across the canonical journal fields and the final tag. Every modified sealed journal fails authentication.

Every truncation cut from zero bytes through one byte before the complete sealed length also fails authentication.

The same valid sealed journal fails under a different HMAC key.

### Valid MAC does not bypass identity or rollback checks

A validly authenticated journal still passes through the existing restart-policy checks.

The regression proves:

- wrong expected operation -> `ForeignOperation`;
- wrong expected journal key identity -> `ForeignKey`;
- an older but correctly authenticated generation -> `GenerationRollback` when the caller supplies the newer minimum generation;
- an older but correctly authenticated nonce high-water -> `NonceRollback` when the caller supplies the newer minimum nonce;
- the current authenticated checkpoint opens successfully.

This distinction is critical: **HMAC prevents undetected modification; HMAC alone does not prevent rollback to an old valid journal.** Rollback resistance still depends on an authoritative generation/nonce floor.

### Determinism and data sensitivity

The same canonical journal and HMAC key produce identical sealed bytes. A later checkpoint produces different sealed bytes.

Determinism is appropriate here because the MAC consumes no nonce and journal confidentiality is not claimed.

## Provider boundary

As with the AES-GCM experiments, real AWS-LC cryptographic execution is native Linux x86_64 research evidence. The generic repository still passes Rust 1.85, i686, and powerpc64 lanes, but 0173 does not claim AWS-LC provider qualification on those cross targets.

## What this closes

Experiment 0173 establishes executable evidence for:

- a real keyed restart-journal authenticator rather than the earlier test-only hash authenticator;
- HMAC-SHA256 over the exact existing canonical journal bytes;
- real tag verification before journal parsing/authority;
- field/tag/truncation/wrong-MAC-key rejection;
- preservation of existing operation/key/generation/nonce rollback checks under a real MAC;
- avoidance of a circular journal-AEAD nonce-durability dependency.

## What remains open

Issue #11 remains open. In particular, Experiment 0173 does **not** establish:

- journal confidentiality;
- a real descriptor-pinned crash-durable journal store;
- file-sync and directory-sync ordering for journal generations;
- restart selection among multiple durable/incomplete journal generation files;
- a durable external anti-rollback anchor against malicious deletion of the newest valid journal generation;
- production provisioning/rotation/lifecycle for the HMAC key or journal `key_id`;
- end-to-end binding between the HMAC key identity and the AES-GCM private-stage key identity;
- physical power-loss/filesystem durability qualification;
- closure of the documented Linux same-UID final identity-check -> unlink race;
- AWS-LC provider qualification outside the native Linux x86_64 experiment.

## Verification

Accepted implementation head before this evidence note:

`12bd943a70b4f6e36cf940d91889dd0349423c4c`

Repository Rust workflow run:

`31794914785`

The accepted implementation head completed successfully on:

- locked dependency-graph verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including the real HMAC regressions;
- Rust 1.85.0 and HTTP-feature MSRV;
- i686 generic-writer portability;
- powerpc64 generic-writer portability;
- concrete and async HTTP source checks;
- versioned S3 source checks;
- deletion-policy reproduction/measurement/catalog checks;
- Rust documentation;
- independent/adversarial parser and Phase 3 model checks;
- EXP-0002/EXP-0003 corpus and amendment verification;
- framing experiment replay.

## Next executable slice

Experiment 0174 should persist authenticated journal generations under a descriptor-pinned private directory using append-only generation files.

Required ordering:

`create-new generation file -> write sealed bytes -> file sync -> directory sync -> generation becomes restart-authoritative`

Restart should perform a bounded descriptor-relative inventory, authenticate candidate generations with the Experiment 0173 HMAC path, require the filename generation to equal the authenticated journal generation, and select the highest fully valid generation. Truncated/tampered canonical journal candidates must never become authority.

The experiment must distinguish ordinary crash recovery from malicious rollback: selecting the highest surviving authenticated generation protects against incomplete writes and stale surviving generations, but deleting the newest durable file can still expose an older valid generation unless an external durable floor exists. That stronger anti-rollback boundary must remain explicit.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
