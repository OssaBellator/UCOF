# Experiment 0143 — Async exact-end full validation over real HTTP

**Status:** non-normative Phase 3 implementation/qualification evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Depends on:** Experiments 0139–0142 / PRs #122–#125

## Purpose

Experiment 0142 proved native async targeted lookup over one strong-version HTTP source. This experiment ports the stricter operation: validate the complete exact-end active snapshot, every reachable directory page, and every active object without hiding an async runtime behind the synchronous source trait.

## Operation boundary

`validate_source_at_async` acquires metadata once, retaining:

```text
exact object length
one strong source version
```

Every later HTTP range is conditioned on that exact version/length and rechecked before parser bytes are accepted.

The operation never invokes recovery.

## Bounded async reader

The full-validation reader mirrors synchronous `SourceReader` limits and stats:

- maximum read operations;
- maximum bytes per read request;
- cumulative source bytes;
- bounded streaming hash block;
- format file/allocation/object/page/depth limits;
- bytes read/hashed and largest-allocation accounting.

Current-commit and object payload hashing remain streamed. The implementation does not cache the whole file or complete commit merely to reuse synchronous validation.

## Full-tree algorithm

The async implementation mirrors `validate_source_at`:

1. authenticate header, exact-end footer, snapshot, parent-link facts, and current commit digest;
2. iteratively walk every reachable page with an explicit stack;
3. authenticate each page digest and reference/range/shape/padding facts;
4. collect every leaf locator and every structural page range;
5. verify footer `page_count_current` against pages emitted in the active commit;
6. sort locators and reject duplicate/out-of-order object IDs;
7. derive/sort object ranges and reject object/object overlap;
8. reject object/structural overlap;
9. authenticate every active object header and complete record digest;
10. return the same `ImmutableReport` and `ImmutableSourceStats` structure as synchronous full validation.

## Report type

`AsyncImmutableSourceStrictReport` adds only source-view facts around the existing strict report:

```text
source_version
source_length
strict: ImmutableSourceStrictReport
```

This keeps the actual validation result/stat structure directly comparable to synchronous `validate_source_at`.

## Differential real-HTTP tests

### Multi-page valid file

The test creates more than one leaf page, then validates the same bytes through:

- synchronous `ImmutableSliceSource + validate_source_at`;
- native async authenticated/retrying Reqwest + `validate_source_at_async`.

The complete strict report, including stats, must match exactly.

The loopback server also asserts one HEAD and verifies that GET count equals accepted async source read operations.

### Version change during tree walk

Metadata returns ETag `"v1"`. The server accepts several range reads, then starts returning 412.

The async validator terminates with:

```text
Conditional(VersionChanged)
```

rather than continuing with a mixed source view.

### Page-count resource limit

A multi-page valid file with `max_pages = 1` must fail with the same source/format page-count limit class as the synchronous policy.

### Corruption

A payload byte is changed without updating authenticated outer bytes. The async validator rejects the file during current-commit authentication before it can report a valid active state.

## Assurance boundary

This experiment establishes full **active-state** validation over the concrete HTTP stack. It does not yet establish:

- verified linked-history traversal over HTTP;
- recovery candidate validation over HTTP;
- freshness/rollback authorization;
- provider-specific cloud-object version/signing semantics.

Those remain separate operations/gates.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest conditional_async_source_full
```

The HTTP feature must also continue compiling on Rust 1.85.0.

## Boundary

This uses the current immutable-successor research microformat. It does not select or change EXP-0003 bytes, D1–D7, FCP status, or epoch allocation.
