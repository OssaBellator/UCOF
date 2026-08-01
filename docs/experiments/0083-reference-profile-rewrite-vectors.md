# Experiment 0083: Reference-Profile Rewrite Vectors

**Status:** Research evidence  
**Date:** 2026-08-01  
**Epoch allocation:** None

## Question

Can the non-normative canonical reference-list dependency profile produce the same retained-object set and exact canonical rewrite bytes in the Rust semantic compactor and an independent Python implementation?

## Construction

Three recipes cover:

1. one root with a two-level dependency chain and one orphan;
2. two caller-unsorted roots sharing one dependency;
3. one reference object with an empty dependency list and one orphan.

Both implementations:

- encode or decode the reference-list payload independently;
- sort and deduplicate roots and dependencies before traversal;
- compute retained and discarded object identifiers, edge count, and maximum depth;
- rebuild only retained objects as one canonical genesis file;
- verify root level, page count, object count, output length, and SHA-256.

The Python implementation uses the clean-room immutable object/page writer and strict validator already present on the reference-profile lineage. Rust uses `semantic_compact` with `CanonicalReferenceListResolver` and `UnknownDependencyPolicy::Reject`.

## Pinned results

| Recipe | Bytes | SHA-256 | Root level | Pages | Objects |
|---|---:|---|---:|---:|---:|
| `single-root-chain` | 16,904 | `87bc7de2d5e2afb51e765bf4694cf3a9a178a605d0f032b8157a4bc1bfc7040e` | 0 | 1 | 4 |
| `two-roots-shared-dependency` | 16,968 | `aee59b41b6a7bf135fc1d741256ea7e6ef121ffb1897a39b646ca2f66b73b715` | 0 | 1 | 5 |
| `empty-reference-root` | 16,728 | `65654ef62675f12db1ed3fde4304eaa1344e2cc25bbb5a2718c51ba3779c5e43` | 0 | 1 | 1 |

The name-plus-raw-digest aggregate is:

`fe20a891b04b90b6df1870e6652eec5d6ddfa91ebc8370f4d6dfa70881a27c84`

## Important boundary

These vectors use the current research layout, including 64-bit identifiers, 88-byte locators, and provisional kinds 100 and 101. They do not adopt an application profile or allocate a wire epoch. The reference-list profile does not define extension preservation, provenance, signature reissuance, large-graph spill, or root-selection authority. Any future identifier, locator, occupancy, payload, or profile change requires regenerated identities and explicit migration evidence.
