# Experiment 0013: Bounded Deterministic Leaf-Entry Sort

- **Status:** Reproducible writer-model evidence
- **Date:** 2026-07-30
- **Related:** FCP-0002, Candidate 1 leaf layout
- **Script:** `tools/experiment_exp0002_external_sort.py`

## Question

Can a deterministic large-file writer order Candidate 1's fixed 88-byte leaf entries without retaining the complete directory ledger in memory?

## Prototype

The experiment generates 200,003 unique object identifiers in a deterministic affine permutation and materializes exact 88-byte leaf-entry-shaped records. It then:

1. buffers at most a configured number of entries;
2. sorts each bounded run by numeric object identifier;
3. writes each run to a temporary fixed-record spill file;
4. performs a k-way heap merge;
5. rejects duplicate, missing, or unordered identifiers while merging;
6. hashes the canonical merged bytes without retaining the complete output.

Two run sizes, 4,096 and 7,777 entries, must produce the same output SHA-256. A separate cross-run duplicate case must fail closed.

## Expected resource shape

| Entries per run | Approximate peak run bytes | Expected runs |
|---:|---:|---:|
| 4,096 | 360,448 | 49 |
| 7,777 | 684,376 | 26 |

Both configurations spill exactly `200,003 × 88 = 17,600,264` entry bytes and merge to the same number of canonical output bytes. Temporary-file metadata and operating-system buffers are outside the stated peak-run figure.

## Finding

A fixed-width Candidate 1 leaf ledger can be ordered deterministically with memory proportional to one run plus one record per open run, rather than total object count.

The output is independent of run size when:

- numeric identifier order is specified exactly;
- records have one canonical fixed-width encoding;
- duplicate identifiers fail closed;
- run boundaries and temporary filenames do not enter output bytes;
- the merge emits only final canonical records.

This closes the algorithmic feasibility question for a basic external sort. It does not yet integrate spill runs with object emission, page packing, append reuse, or failure-safe output publication.

## Security and operational requirements

A production external-sort writer must additionally bound:

- run count and simultaneously open files;
- spill bytes and destination free space;
- path and permission handling for temporary files;
- total comparisons and merge work;
- object-header and payload ledger size;
- cleanup after source, spill, or output failure;
- sensitive metadata leakage into temporary storage;
- concurrent writers and temporary-name collisions.

Temporary entries may reveal object identifiers, kinds, sizes, locators, and digests. Encrypted-profile writers require an explicit policy for protected spill storage and secure cleanup; ordinary deletion is not guaranteed physical erasure.

A writer must not publish the final footer until all object, page, snapshot, and footer inputs have been emitted successfully. A spill file is never a valid UCOF checkpoint.

## Remaining integration work

Before FCP-0002 Review, the reference writer still needs a byte-producing prototype that:

1. streams or spools object records under bounded policy;
2. externally sorts authenticated leaf locators;
3. packs final leaf and internal pages deterministically;
4. shares final batch paths if page reuse is retained by a later byte candidate;
5. handles disk-full and I/O failures without publishing an invalid footer;
6. reports spill and output accounting.

Experiment 0011 also shows that Candidate 1's current page-sequence equality prevents historical page reuse, so external sorting alone cannot solve append amplification under the present bytes.

## Reproduction

```console
python3 tools/experiment_exp0002_external_sort.py
```

The script asserts exact spill and output sizes, bounded peak run memory, run counts, chunk-size-independent output, complete identifier coverage, and duplicate rejection.
