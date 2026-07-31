# Experiment 0057 — Spill ownership and publication state machine

**Status:** executable policy model, not production filesystem implementation  
**Date:** 2026-07-31

## Question

Can spill cleanup and no-overwrite publication expose the three required external outcomes without allowing ownership mistakes, budget overflow, or cleanup errors to produce false publication claims?

## Model

`SpillPublicationSession` requires a non-zero caller-supplied 256-bit operation ownership token and one explicit confidentiality policy. It independently accounts staged bytes, staged files, and cleanup actions with checked arithmetic.

The modeled same-filesystem sequence is:

1. private staging;
2. complete output validation;
3. staged file synchronization;
4. no-overwrite destination link or rename;
5. destination directory synchronization;
6. private staged-name retirement.

The platform adapter reports the no-overwrite primitive as one of: destination exists, definitely not created, created, or indeterminate. The model never guesses across that boundary.

## External outcomes

- Before a destination is known to exist, failures remain `NotPublished`.
- An ambiguous link result or a directory-sync failure after a successful link becomes `PublicationIndeterminate`.
- Successful destination-directory synchronization becomes `PublishedAndDurable` under the selected platform contract.
- Later cleanup failure does not downgrade durable publication.

## Evidence

Unit tests cover:

- ownership-token mismatch before mutation;
- independent staged-file and staged-byte limits;
- pre-existing destination without publication;
- indeterminate no-overwrite link reporting;
- post-link directory-sync failure;
- cleanup failure after durable publication;
- explicit encrypted-spill-required policy retention.

## Assurance boundary

This model performs no filesystem operations and supplies no encryption. It does not establish private directory permissions, no-follow opens, hard-link checks, same-filesystem identity, actual no-overwrite semantics, file or directory durability, nonce uniqueness, authentication, crash restart, or power-loss behavior.

Production advancement still requires a platform adapter, encrypted segment framing, fault injection at every transition, and qualification on each supported filesystem class. The model's purpose is to make unsafe outcome collapsing and ownership-free cleanup unrepresentable in that adapter's policy layer.
