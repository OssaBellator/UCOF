# Experiment 0036: Private Spill Lifecycle and Atomic Publication

- **Status:** Reproducible filesystem-publication prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0016, 0028, and 0035
- **Script:** `tools/experiment_exp0002_spill_publication.py`

## Question

Can a descriptor-bounded spill writer enforce private staging, disk budgets, create-new publication, and honest failure outcomes before and after the atomic filesystem publication point?

## Pipeline

The experiment processes 4,097 locator records through:

1. bounded sorted spill runs;
2. descriptor-limited staged merge;
3. canonical immutable page emission;
4. comparison with a directly sorted byte baseline;
5. file `fsync`;
6. atomic create-new hard-link publication on the same filesystem;
7. parent-directory `fsync`.

The published artifact is the deterministic directory-page output used by the writer experiments. It is not a complete successor file.

## Private staging

Each operation uses a randomly named private staging directory. The prototype requires:

- no group or other permissions on the staging directory;
- no group or other permissions on spill, merge, reference, or output files;
- explicit cleanup on ordinary failure;
- explicit startup cleanup for abandoned staging directories left by simulated process death.

Private names and permissions reduce accidental disclosure but do not provide encrypted spill or guaranteed physical erasure.

## Publication boundary

Publication uses a hard link from the fully written and file-synchronized staging artifact to a previously absent destination on the same filesystem.

This operation is:

- atomic with respect to destination visibility;
- create-new rather than overwrite;
- rejected when the destination already exists.

The experiment injects failures after run creation, merge, page emission, file synchronization, and destination linking.

- Before the link, failure is honestly reported as **not published** and the destination is absent.
- After the link, failure is **indeterminate to the caller but published on disk**. The destination exists and matches the validated expected artifact.

A writer must not report the post-link case as definitely unpublished merely because the function did not return success.

## Resource behavior

A disk-budget check is applied after every material staging phase. A deliberately tiny budget fails before publication and leaves no destination. Descriptor fan-in remains bounded by the staged merge limit.

## Findings

1. Publication state changes at the atomic filesystem operation, not at function return.
2. Post-publication errors require an indeterminate or inspectable result state.
3. Create-new semantics prevent accidental replacement of an existing destination.
4. Private staging and abandoned-staging cleanup are separate requirements.
5. Disk, descriptor, byte, run, and merge-pass limits must be configured together.

## Boundaries

Hard-link publication is filesystem-specific and requires staging and destination on the same filesystem. A production writer must define behavior for unsupported filesystems, cloud object stores, network filesystems, directory-sync failures, cancellation, encrypted profiles, secure deletion expectations, and final complete-file validation. This experiment does not claim crash consistency on every operating system or storage device.

## Reproduction

```console
python3 tools/experiment_exp0002_spill_publication.py
```
