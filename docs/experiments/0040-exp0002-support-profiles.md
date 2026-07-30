# Experiment 0040: Jointly Satisfiable Support Profiles

- **Status:** Reproducible resource-policy evidence
- **Date:** 2026-07-30
- **Related:** ADR-0015 and Experiment 0019
- **Script:** `tools/experiment_exp0002_support_profiles.py`

## Question

Can a support profile be expressed as one jointly satisfiable tuple rather than a list of independent maxima that no validator can exercise together?

## Conservative model

For an immutable successor using 16 KiB pages, 88-byte leaf entries, and 64-byte internal entries, the experiment derives page count from object count and uses conservative full-validation bounds.

Read operations include:

- fixed header, footer, snapshot, and bookkeeping operations;
- one fixed-header request per object;
- bounded object-hash requests at the profile's maximum object and request sizes;
- one request per directory page;
- one bounded current-commit hash pass.

Read bytes reserve three file lengths:

1. one complete commit hash pass;
2. one complete active object/page validation pass;
3. footer, snapshot, lookup, history, and implementation reread allowance.

Hash bytes reserve two file lengths for current-commit and active object/page hashing.

These are conservative research equations, not normative requirements. A production implementation may prove tighter bounds, but a profile must publish its own satisfiable accounting.

## Example profiles

### Research small

- maximum file: 64 MiB;
- maximum objects: 100,000;
- maximum object: 1 MiB;
- request and hash chunk: 1 MiB;
- maximum read operations: 210,000;
- maximum bytes read: 192 MiB;
- maximum bytes hashed: 128 MiB;
- maximum single allocation: 2 MiB;
- verified history depth: 64;
- recovery suffix: 16 MiB.

### Research medium

- maximum file: 4 GiB;
- maximum objects: 1,000,000;
- maximum object: 16 MiB;
- request and hash chunk: 1 MiB;
- maximum read operations: 17,100,000;
- maximum bytes read: 12 GiB;
- maximum bytes hashed: 8 GiB;
- maximum single allocation: 2 MiB;
- verified history depth: 1,024;
- recovery suffix: 64 MiB.

### Research large

- maximum file: 64 GiB;
- maximum objects: 10,000,000;
- maximum object: 64 MiB;
- request and hash chunk: 4 MiB;
- maximum read operations: 170,100,000;
- maximum bytes read: 192 GiB;
- maximum bytes hashed: 128 GiB;
- maximum single allocation: 8 MiB;
- verified history depth: 4,096;
- recovery suffix: 256 MiB.

The names describe executable research examples, not mandatory product tiers.

## Rejected independent-default tuple

The experiment also evaluates a tuple retaining a ten-million-object maximum but only one million read operations. It fails before format semantics because even optimistic full validation requires far more operations.

This confirms ADR-0015: implementation defaults are policy ceilings, not a conformance profile.

## Required profile properties

A future support profile should publish together:

- maximum file and object bytes;
- object and page counts;
- page depth;
- request and allocation sizes;
- read-operation and cumulative-read limits;
- hash limits;
- history and recovery limits;
- spill and output limits;
- exact boundary vectors and refusal classifications.

A file refused solely because it exceeds a declared support profile is not thereby malformed.

## Findings

1. Profile limits must be selected and tested as one satisfiable tuple.
2. Request size, maximum object size, and read-operation count are strongly coupled.
3. File size, bytes-read, and hash budgets are also coupled by validation strategy.
4. Directory page count is derivable from object count only after page and locator choices are fixed.
5. Normative profiles should be justified by implementation evidence and boundary vectors, not copied from independent safety defaults.
