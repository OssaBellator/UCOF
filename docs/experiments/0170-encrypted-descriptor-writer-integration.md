# Experiment 0170 — Encrypted descriptor writer integration

**Status:** non-normative Phase 3 production-writer confidentiality evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0157, 0159, 0166

## Purpose

Experiment 0170 moves real authenticated encryption into the bounded source-writer execution path rather than leaving confidentiality as a model-only contract.

The accepted integration encrypts the retained sorted source-descriptor stage with AES-256-GCM, obtains nonces only from the crash-safe Experiment 0157 lease contract, authenticates the complete stage before the first output byte, and then feeds decrypted descriptors into the same canonical emission loop used by the plaintext bounded candidate.

The experiment deliberately does not claim complete private-stage confidentiality yet. The bounded sorter's temporary run files, locator stage, and page-reference stages remain separate follow-on gates.

## Cryptographic provider boundary

The real AEAD adapter uses `aws-lc-rs 1.18.0` with default compatibility features disabled and the existing non-FIPS `aws-lc-sys` backend selected explicitly.

The AWS-LC dependency is target-gated to the native Linux x86_64 test experiment. This is intentional:

- the generic writer still passes Rust 1.85.0, i686, and powerpc64 checks;
- the repository's cross-target lanes do not provide the C cross-toolchains needed to qualify AWS-LC itself on those targets;
- therefore this experiment does **not** claim AWS-LC provider qualification on i686 or powerpc64.

The locked graph remains authoritative and `--locked` is preserved.

## Nonce authority

The encryption module includes the exact Experiment 0157 nonce-lease implementation rather than copying its counter logic.

An encryption session can exist only after:

1. a nonce lease is reserved from the current durable high-water state;
2. that lease's high-water generation is accepted as durably committed by the lease contract;
3. the resulting `ActiveNonceLease` is transferred into the opaque encryption session.

Writer code does not receive a raw nonce counter allocator.

Executable cases prove:

- a non-durable reservation cannot create an encryption session;
- a lease that is too short fails before encrypted-stage creation and consumes no nonce;
- one counter is consumed for every encrypted descriptor frame;
- the exact lease is exhausted after the expected record count;
- the durable high-water mark advances past the entire committed lease.

Physical journal durability remains external to this experiment; the durable-commit boolean is still the Experiment 0157 boundary, not a filesystem journal implementation.

## Encrypted descriptor frame

Every 64-byte source descriptor is persisted as one fixed 92-byte private frame:

- 12-byte nonce;
- 64-byte AES-256-GCM ciphertext;
- 16-byte authentication tag.

The nonce is:

- a four-byte session prefix;
- followed by the leased 64-bit counter in big-endian form.

AAD binds:

- the Experiment 0170 descriptor domain;
- operation identity;
- durable journal generation;
- descriptor sequence;
- nonce counter;
- exact plaintext descriptor width.

The reader requires the exact expected nonce sequence before authentication and then requires an exact 64-byte plaintext descriptor after authentication.

## Writer integration

The existing bounded writer still performs the same source metadata preflight and bounded external sort.

After the sorter produces the retained canonical 64-byte descriptor stage, Experiment 0170 performs:

`plaintext sorted descriptor stage -> lease-backed AES-GCM transcode -> full authenticated prevalidation -> canonical object/tree emission`

The plaintext retained descriptor stage is consumed by the transcode and removed when its `FixedStage` drops.

Both plaintext and encrypted candidates feed a single `PreparedDescriptorReader` emission loop. The encryption experiment therefore does not maintain a second canonical writer implementation.

## Pre-output authentication rule

The complete encrypted descriptor stage is decrypted, authenticated, descriptor-decoded, and exact-end checked before the shared emission loop writes the UCOF file header.

A controlled regression flips one encrypted-stage byte between transcode and prevalidation and proves:

- AES-GCM authentication fails;
- no output byte is written;
- the already-issued nonce lease remains burned rather than reused;
- temporary files are cleaned up.

The stage is read again during actual emission, so later corruption is still detected while streaming.

## Canonical output equivalence

A 401-object regression compares:

- the existing canonical source writer;
- the plaintext bounded end-to-end candidate;
- encrypted private execution A;
- encrypted private execution B.

The two encrypted executions use the same key and operation identity but different private nonce prefixes.

The regression proves:

- both encrypted executions produce the exact canonical UCOF bytes;
- both encrypted executions return the exact canonical writer report;
- the plaintext bounded candidate still produces those same bytes/report;
- each encrypted descriptor stage is exactly `401 * 92` bytes;
- the two private ciphertext-stage SHA-256 digests differ;
- the canonical output remains independent of private encryption randomness.

Private ciphertext variability therefore does not leak into UCOF compatibility bytes.

## Private-storage quota accounting

The previous production-writer storage plan priced:

- sorter spill + plaintext descriptor stage;
- plaintext descriptor + locator stage;
- locator + first page-reference stage;
- adjacent page-reference levels.

Experiment 0170 adds the two new overlap windows:

- plaintext descriptor + encrypted descriptor during transcode;
- encrypted descriptor + locator stage during object emission.

For a synthetic four-object geometry with the sorter's live-spill term held to 256 bytes, the regression proves:

- plaintext descriptors: 256 bytes;
- encrypted descriptors: 368 bytes;
- plaintext + encrypted transcode overlap: 624 bytes;
- encrypted descriptor + locator overlap: 656 bytes;
- required private capacity: 656 bytes.

A separate executable quota case uses a physically viable sorter budget and proves:

- exact computed encrypted private capacity succeeds;
- one byte less fails before writer I/O;
- that preflight failure consumes no nonce from the already-active lease.

The arithmetic proof and the real sorter-cap proof are deliberately separate; an artificially tiny sorter cap is not treated as an executable storage configuration.

## What this closes

Experiment 0170 establishes real writer-path evidence for:

- a vetted AES-256-GCM implementation rather than a test-only cipher model;
- locked dependency integration;
- crash-safe lease capability in front of the real seal operation;
- fixed-width encrypted retained descriptors;
- authenticated prevalidation before output;
- canonical output/report independence from private encryption;
- explicit encrypted-stage storage expansion accounting.

## What remains open

Issue #11 remains open. In particular, Experiment 0170 does **not** establish:

- encrypted bounded-sorter run files — sorter runs still contain clear 64-byte descriptor payloads with clear sort keys;
- encrypted locator stages;
- encrypted page-reference stages;
- a production key-management or key-provisioning contract;
- persistent authenticated storage of key identity, nonce prefix, lease high-water state, expected descriptor-stage identity, or anti-rollback state;
- a real crash-durable journal implementation for Experiment 0157/0159 state;
- physical power-loss/filesystem durability qualification;
- closure of the documented same-UID Linux final identity-check -> unlink race;
- AWS-LC provider qualification outside the native Linux x86_64 experiment.

## Verification

Accepted implementation head before this evidence note:

`1310be45774b1b7e61deb6e2ae1a2b8220f143d1`

Repository Rust workflow run:

`31791756575`

The accepted implementation head completed successfully on:

- locked dependency-graph verification;
- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0170 regressions;
- concrete HTTP transport and async source tests;
- versioned S3 source adapter tests;
- Rust 1.85.0 MSRV and HTTP-feature MSRV;
- i686 portability checks for the generic writer;
- powerpc64 portability checks for the generic writer;
- deletion-policy reproduction and measurement;
- Rust documentation;
- independent/adversarial parser and Phase 3 model checks;
- EXP-0002/EXP-0003 corpus and amendment verification;
- framing experiment replay.

## Next executable slice

Experiment 0171 should encrypt the bounded sorter's temporary run payloads while preserving the existing single sorter implementation.

The intended architecture is a frame-codec seam in the current bounded sorter:

- keep the 64-bit sort key visible for bounded merge ordering;
- authenticate that clear key as AAD;
- encrypt the fixed descriptor payload with a freshly leased nonce for every persisted run frame, including merge outputs;
- decrypt/authenticate each frame before it enters the merge heap;
- keep duplicate/order checks in the existing sorter logic;
- charge the larger encrypted run frame to every live/read/write spill budget;
- prove plaintext and encrypted sorter outputs/reports are logically equivalent while private spill ciphertext differs.

Sorter-run encryption is the next priority because otherwise descriptor confidentiality remains incomplete even though the retained descriptor stage is encrypted.

## Governance boundary

This is private-writer implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify immutable-successor wire bytes, or make a compatibility promise.
