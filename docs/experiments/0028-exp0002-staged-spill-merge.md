# Experiment 0028: Descriptor-Bounded Staged Spill Merge

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0013 and 0016
- **Script:** `tools/experiment_exp0002_staged_spill_merge.py`

## Question

Can the spill-backed deterministic writer cap open input files through staged multi-pass merging while preserving the exact canonical immutable-page output?

## Boundary fixture

The experiment generates 200,003 exact 88-byte locator records using 512-entry initial sort runs. This creates 391 initial run files and a 45,056-byte peak sort buffer.

Three maximum-open-input policies are compared:

- 4 input runs;
- 8 input runs;
- 32 input runs.

One additional output file is open during each merge group.

## Staged merge

Each pass:

1. groups at most the configured number of sorted runs;
2. performs a heap merge;
3. rejects duplicate or decreasing identifiers;
4. writes one next-pass run;
5. removes consumed input runs;
6. carries an unpaired final run without rewriting it.

The final run must contain exactly identifiers `1..200003`. It then feeds the immutable leaf and internal page emitter from Experiment 0016.

## Required evidence

The executable requires:

- 391 initial runs for every policy;
- five merge passes at fan-in 4;
- three merge passes at fan-in 8;
- two merge passes at fan-in 32;
- peak open files no greater than fan-in plus one output;
- duplicate rejection even when duplicates meet only in a later pass;
- identical root locator, root digest, page bytes, output length, and whole-output SHA-256 versus the directly sorted baseline.

## Trade-off

Lower fan-in reduces descriptor pressure but increases complete spill read/write passes. A production policy must jointly bound:

- initial run entries and memory;
- open input and output files;
- merge passes;
- bytes read and written;
- temporary disk occupancy;
- inodes and directory entries;
- wall-clock deadline and cancellation work;
- cleanup after process or host failure.

No single descriptor limit is meaningful without the corresponding byte and pass budgets.

## Security and durability

The staged merger still requires production rules for:

- private temporary directories and restrictive permissions;
- symlink, path-replacement, and race resistance;
- storage-full and short-write handling;
- integrity if spill files survive outside a trusted process boundary;
- cancellation cleanup;
- crash residue discovery and reclamation;
- durable synchronization before exact-end publication.

## Finding

Descriptor-bounded staged merging is compatible with deterministic canonical page emission. The cost is explicit additional spill I/O, not a change to the resulting UCOF page identity.

## Reproduction

```console
python3 tools/experiment_exp0002_staged_spill_merge.py
```
