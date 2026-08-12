# Experiment 0034: Authenticated Roots, Capabilities, and Extensions

- **Status:** Reproducible metadata-object prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0020, 0031, and 0033
- **Script:** `tools/experiment_exp0002_immutable_page_metadata.py`

## Question

Can roots, required and optional capabilities, and future extension records become authenticated inputs to an immutable-page snapshot while preserving a strict distinction between structural integrity and semantic interpretability?

## Catalog object

The prototype reserves one experimental object identifier and kind for a catalog object. The catalog payload contains:

- a fixed magic and version;
- a sorted unique non-empty root-object list;
- sorted unique capability records with per-capability required criticality;
- one canonical length-delimited extension block from Experiment 0020.

The catalog is an ordinary complete object. Its complete record digest is authenticated by an immutable leaf entry, page path, snapshot, and commit. No unauthenticated side channel or footer-only hint is used.

The reserved identifier and kind are local to this disposable experiment and are not registry allocations.

## Validation

Catalog validation first performs complete active-snapshot validation, then requires:

- exactly one active catalog object;
- the expected experimental kind;
- exact catalog lengths and zero reserved fields;
- sorted unique non-zero roots and capabilities;
- every declared root to reference an active object;
- canonical extension ordering, flags, lengths, and zero padding.

The catalog object itself cannot be a root.

## Interpretability

Known capabilities may be interpreted. Unknown optional capabilities are reported and retained. Unknown required capabilities are also reported, but they prevent a `fully interpretable` result.

The underlying file and catalog can still be structurally and cryptographically verified. Tools must not erase that evidence merely because the application lacks one required capability, and they must not describe the content as fully interpretable.

## Rewrite behavior

The experiment proves two distinct cases:

1. replacing an unrelated ordinary object reuses the exact historical catalog object and locator;
2. replacing the catalog to update one known extension preserves every unknown optional extension record byte-for-byte.

A replacement with a missing root or duplicate roots remains cryptographically well-formed but fails catalog semantics after outer integrity checks.

## Findings

1. Roots and capability declarations can be authenticated as an ordinary immutable object.
2. Structural verification and full interpretability require separate result fields and API language.
3. Required criticality belongs on each capability declaration rather than in a global unknown-data policy.
4. Unknown optional extensions can survive authenticated catalog replacement byte-for-byte.
5. Root existence is an active-snapshot semantic invariant independent of object and page digest validity.

## Boundaries

This is not a final catalog representation. The experiment does not select a permanent catalog identifier, object kind, capability namespace, profile model, root ordering policy beyond numeric object identifiers, or extension registry. It does not yet integrate the catalog with bounded source lookup, history-retention rules, cross-language vectors, or recovery.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_metadata.py
```
