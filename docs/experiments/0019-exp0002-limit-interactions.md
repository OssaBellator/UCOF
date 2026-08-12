# Experiment 0019: EXP-0002 Limit Interactions

- **Status:** Reproducible
- **Date:** 2026-07-30
- **Related:** FCP-0002, source and recovery APIs
- **Script:** `tools/experiment_exp0002_limit_interactions.py`

## Question

Can the current Candidate 1 implementation defaults be promoted directly into one normative minimum support profile?

## Defaults examined

The experiment records the current defaults from `ValidationLimits`, `Exp0002SourceLimits`, and `Exp0002SourceRecoveryLimits`, including:

- 16 GiB file, commit, and payload ceilings;
- 32 GiB hashed-byte and source-read ceilings;
- 1,000,000 pages;
- depth 32;
- 10,000,000 objects;
- 1,000,000 source read operations;
- 64 KiB maximum read requests;
- 16 MiB recovery scan;
- 1,024 candidate validations;
- 64 GiB cumulative candidate reads.

These values were chosen as implementation-local hostile-input ceilings. They were not designed as a jointly satisfiable interoperability class.

## Object-count and read-operation conflict

Ten million objects require 54,268 Candidate 1 directory pages and 889,126,912 directory bytes. Even with zero-byte payloads, 48-byte object headers plus the directory require at least 1,369,126,912 bytes.

That physical size fits below the 16 GiB file ceiling. However, full source validation requires at least one object-related read per object in an unrealistically optimistic model. Ten million reads exceed the default one-million read-operation budget by 10x.

Therefore `max_objects = 10,000,000` and `max_read_operations = 1,000,000` cannot both describe the boundary of one full source-validation support class.

## Recovery interactions

A 16 MiB scan using 64 KiB requests needs only 256 reads, well below the 4,096 scan-read ceiling.

In contrast, the 64 GiB cumulative candidate-read ceiling can fund only four complete 16 GiB candidate validations. The nominal 1,024 candidate-validation ceiling is reachable only for much smaller candidates.

Both bounds are useful attack controls, but neither should be described alone as guaranteed recovery capacity.

## Snapshot interactions

One million roots consume 8,000,000 bytes. Two arrays of 65,536 capability identifiers consume 1,048,576 bytes. Including the Candidate 1 snapshot header, those configured maxima fit under the 16 MiB snapshot ceiling.

This relationship is internally satisfiable, unlike the object/read-operation pair.

## Corpus coverage

The largest pinned valid Candidate 1 vector is 85,528 bytes with 400 objects. It exercises a tiny fraction of the current policy defaults. Passing that corpus does not demonstrate support near any proposed large-file conformance boundary.

## Findings

1. Current defaults are independent safety ceilings, not one coherent normative support class.
2. Numeric support requirements must be selected jointly across file size, objects, pages, reads, bytes read, bytes hashed, and allocation.
3. A count ceiling can be unreachable because a byte or operation ceiling binds first.
4. Recovery capacity must be stated as a multidimensional work envelope, not only candidate count.
5. Resource-limit refusal is not malformed-file rejection and must remain a distinct result.
6. Boundary conformance vectors must exercise the selected support class rather than only small functional examples.

## Proposed decision method

Before FCP-0002 Review, define one or more explicit support profiles. Each profile should provide jointly satisfiable minima for:

- file and commit bytes;
- objects and payload bytes;
- pages and depth;
- roots and capabilities;
- read operations, bytes read, and request size;
- bytes hashed;
- recovery scan and candidate work;
- diagnostics and output work.

For every profile, publish constructed boundary vectors or virtual-source tests proving that a conforming implementation can satisfy all minima simultaneously.

Implementation deployments may choose higher or lower policy ceilings, but implementations claiming a profile must not silently advertise unreachable combinations.

## Reproduction

```console
python3 tools/experiment_exp0002_limit_interactions.py
```
