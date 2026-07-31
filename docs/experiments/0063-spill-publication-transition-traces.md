# Experiment 0063 — Independent spill publication transition traces

**Status:** cross-implementation publication-policy evidence  
**Date:** 2026-07-31

## Question

Can spill publication outcome, ownership, resource, cleanup, and transition decisions be reproduced from machine-readable traces without invoking the Rust state machine or performing filesystem operations?

## Contract

`publication-traces.json` defines operation limits and ordered events for:

- staging owned files;
- complete output validation;
- staged-file synchronization;
- no-overwrite destination linking;
- destination-directory synchronization;
- private-name retirement;
- cleanup of other owned artifacts.

Every trace pins final stage, external publication outcome, staged bytes, staged files, cleanup actions, and any terminal error.

## Independent verifier

`verify_phase3_spill_publication_traces.py` implements the transition policy directly in Python:

- reject ownership mismatch before mutation;
- independently limit staged bytes, staged files, and cleanup work;
- reject invalid transition order;
- keep destination-exists and definitely-not-created outcomes as not published;
- report an ambiguous link or post-link directory-sync failure as publication indeterminate;
- report durable success only after destination-directory synchronization;
- preserve durable success across later cleanup failure or cleanup-budget exhaustion;
- evaluate every trace twice and pin an aggregate canonical-result SHA-256.

## Cases

- durable publication and staged-name retirement;
- destination already exists;
- destination definitely not created;
- indeterminate no-overwrite link;
- indeterminate directory synchronization after link creation;
- cleanup failure after durable publication;
- wrong owner before mutation;
- staged-byte limit;
- staged-file limit;
- staged-file synchronization failure;
- cleanup-budget exhaustion after durable publication;
- invalid transition order.

## Assurance boundary

These traces validate publication-policy state and reporting only. They establish no secure directory handle, no-follow open, file type, hard-link count, same-filesystem identity, actual no-overwrite primitive, file or directory synchronization semantics, encryption, nonce management, crash restart, or power-loss durability. Platform qualification and transition fault injection against real filesystem adapters remain mandatory.
