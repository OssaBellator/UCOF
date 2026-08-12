# Experiment 0033: Bounded Source Access for Immutable-Page Objects

- **Status:** Reproducible bounded source prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0012, 0031, and 0032
- **Script:** `tools/experiment_exp0002_immutable_page_object_source.py`

## Question

Can the successor microformat perform exact-end validation and targeted object lookup through bounded random-access reads without materializing the complete file or unrelated payloads?

## Fixture

The fixture contains 2,000 complete objects. One unchanged object has a 1 MiB payload in the genesis prefix. A later snapshot replaces object 1 with a small payload while continuing to reference the large historical object.

The source adapter records and limits:

- cumulative bytes read;
- read operations;
- largest request;
- commit bytes hashed;
- object bytes hashed;
- pages read;
- objects hashed;
- payload bytes materialized.

Every read checks its range, per-request maximum, cumulative byte budget, and operation budget before returning bytes.

## Targeted lookup

Targeted lookup validates:

- file header and exact-end footer;
- active snapshot digest and linkage to the parent footer;
- the complete current commit span in bounded hash blocks;
- the authenticated root and one directory path;
- the selected locator and complete selected object digest.

The selected payload may be returned to the caller. Unrelated payloads are not materialized or hashed. Absence outside the root range is reported after active commit and root authentication.

The lookup report is intentionally not a full-file validation result. In particular, it does not claim that unrelated active objects or pages outside the selected path were read.

## Full active validation

Full validation additionally:

- traverses every active immutable page;
- enforces page and object-count limits;
- checks global object ordering and root range;
- checks active object/structure and object/object non-overlap;
- streams and authenticates every object reachable from the active directory;
- materializes no payloads.

## Adversarial scope test

The experiment mutates the unrelated 1 MiB historical payload after the latest snapshot is published.

- targeted lookup of object 1 still succeeds because that payload is outside the latest commit span and outside the selected object/path assurance scope;
- full active validation rejects the mutation at the historical object's digest.

This is a required API distinction, not a fallback behavior.

## Findings

1. Bounded source lookup can authenticate one current object without reading unrelated historical payloads.
2. Full active validation must authenticate every object reachable from the active directory, including reused historical objects.
3. Source-read, hash-work, page, object, and payload-materialization counters should remain separate.
4. Limits must fail before the next excess read is issued.
5. Targeted lookup, full active validation, and verified history require distinct result types and user-facing language.

## Boundaries

The prototype uses an in-memory source implementation to model random access. It does not yet implement conditional HTTP reads, asynchronous coalescing, cancellation, recovery scanning, cross-language source readers, or global physical non-overlap claims for targeted lookup.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_object_source.py
```
