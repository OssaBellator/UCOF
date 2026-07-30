# Experiment 0016: Spill-Backed Immutable Page Emission

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** FCP-0002, Experiments 0013, 0014, and 0015
- **Script:** `tools/experiment_exp0002_spill_page_emission.py`

## Question

Can a deterministic large writer take bounded external-sort output and emit canonical immutable directory pages without materializing the complete locator ledger or a complete level of page references in memory?

## Input and geometry

The experiment uses:

- 200,003 deterministic locator-shaped records;
- exact 88-byte Candidate 1 leaf-entry bytes;
- a deterministic affine permutation as unsorted input;
- 16 KiB immutable pages;
- 185 leaf entries per page;
- 255 internal child references per page;
- exact 64-byte spilled page-reference records.

The expected tree contains:

```text
leaf pages:       1,082
level-1 pages:        5
root pages:           1
total pages:      1,088
depth:                3
```

## Pipeline

1. Generate unsorted locator records in bounded in-memory runs.
2. Sort each run and reject duplicate identifiers inside the run.
3. Write exact fixed-width run records to temporary files.
4. K-way merge the runs while enforcing globally increasing, gap-free identifiers.
5. Fill one canonical leaf-page buffer at a time.
6. Write leaf pages directly to the output file.
7. Spill fixed-width leaf references rather than retaining the whole level.
8. Stream each reference level in groups of at most 255 into canonical internal pages.
9. Repeat until one root reference remains.
10. Compare root identity and complete page-file SHA-256 across two run sizes and a directly sorted baseline.

## Bounded memory

The experiment keeps only:

- one sort run;
- one merge heap entry per open run;
- one 185-entry leaf buffer;
- one 255-reference internal-page group;
- fixed bookkeeping.

The page output and page-reference levels are stored on disk. The complete locator set and complete page-reference level are never held in memory by the emission stage.

## Determinism requirements

The executable checks require:

- identical emitted page bytes for 4,096-entry and 7,777-entry sort runs;
- identical root locator and digest for both spill configurations;
- equality with a directly sorted canonical baseline;
- exact page counts and output length;
- exact locator spill accounting;
- duplicate rejection across different spill runs.

## Security and operational boundaries

This prototype proves deterministic bounded data flow, not a complete secure spill subsystem.

A production writer must additionally define:

- private temporary-directory creation and file permissions;
- cleanup after success, cancellation, process failure, or host crash;
- storage-space, inode, descriptor, run-count, merge-pass, and total-I/O budgets;
- staged merging when all runs cannot be open simultaneously;
- confidentiality requirements for unencrypted locator metadata;
- integrity checks for untrusted or externally persisted spill files;
- synchronization and durable publication policy;
- interaction with page splits, deletions, updates, and retained history.

## Interpretation

A bounded deterministic writer does not need to retain the full directory ledger in memory. Exact sorted locator records can flow directly into canonical immutable pages, while fixed-width page references can themselves be spilled level by level.

This closes the algorithmic gap between Experiment 0013's external sorter and Experiment 0015's immutable-page byte prototype. It does not yet define a complete successor epoch or production writer API.

## Reproduction

```console
python3 tools/experiment_exp0002_spill_page_emission.py
```
