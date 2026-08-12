# Experiment 0012: EXP-0002 Cold-Cache HTTP Range Access

- **Status:** Reproducible localhost benchmark
- **Date:** 2026-07-30
- **Related:** FCP-0002, Phase 3 source APIs
- **Script:** `tools/experiment_exp0002_http_range.py`

## Question

How do targeted authenticated lookup and full strict validation differ in HTTP Range requests and transferred bytes when the active snapshot references a large unrelated historical payload?

## Scenario

The benchmark constructs:

1. a genesis file containing a small root object and an unrelated 1 MiB object;
2. an append commit adding one small object and retaining all three objects in the active directory;
3. a local HTTP/1.1 server that accepts one explicit byte range per request and records response bytes and ranges;
4. a cold client with no range cache and 64 KiB hash blocks.

The large object is historical relative to the active append commit. This is essential: a genesis commit digest covers genesis payload bytes, while an append commit digest covers only bytes appended after the previous footer.

Two independent operations are measured:

- **targeted lookup of object 1:** authenticate the bootstrap, exact-end footer, current append commit, snapshot, parent footer, one directory path, and object 1;
- **full strict validation:** perform the same active-state checks, traverse every directory page, and hash all three referenced object records.

## Assertions

The benchmark fails unless:

- targeted lookup makes no range request overlapping the 1 MiB historical object;
- full strict validation does request that object;
- targeted lookup transfers less than full validation;
- targeted lookup transfers less than one quarter of the large payload size;
- full validation transfers more than 1 MiB;
- targeted lookup hashes one object and full validation hashes three.

Elapsed time is printed but not asserted because shared CI timing is noisy. Request count and transferred bytes are deterministic for one Python and block-size version.

## Interpretation

Candidate 1's assurance split is observable over a real range transport:

- targeted lookup can avoid unrelated historical payloads because leaf object digests authenticate historical records independently of the current append commit;
- full strict validation must read every object referenced by the active directory;
- both operations must still hash the complete current commit, so append size directly affects targeted lookup cost;
- request coalescing and caching can reduce request count but must not alter verification scope.

## Security and source-stability implications

A remote source can change between range requests. The localhost server is immutable for one benchmark, so this experiment does not solve remote mutation.

A production range contract must define at least one stable-view mechanism, such as:

- an immutable object version identifier;
- a strong ETag checked on every request;
- an application-provided snapshot token;
- a single immutable content-addressed object;
- fail-closed retry rules when version evidence changes.

Range readers must not silently combine bytes from different source versions. HTTP status, `Content-Range`, response length, and requested range must be checked before hashing.

## Boundaries

This benchmark does not model:

- internet latency, packet loss, TLS, proxies, redirects, or CDN caches;
- concurrent mutation;
- multi-range requests;
- speculative prefetch;
- page or object decompression;
- persistent client caches;
- cloud billing or minimum request charges.

It is a reproducible transport-level baseline, not a production performance claim.

## Reproduction

```console
python3 tools/experiment_exp0002_http_range.py
```

The script prints a Markdown row and JSON record for each assurance mode, the archive size, the large historical object range, and whether each mode read that range.
