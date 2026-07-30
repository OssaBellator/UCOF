# ADR-0016: Keep Immutable-Successor Validity, History, and Recovery as Separate Rust Assurance Scopes

- **Status:** Accepted for non-normative successor experimentation
- **Date:** 2026-07-30
- **Scope:** `crates/ucof-experiments::immutable_successor`

## Context

The immutable-page successor experiments now have a reusable Rust slice implementation that reproduces the pinned genesis, append, and multi-level byte identities. The same implementation needs strict current-state validation, linked-history verification, and interrupted-publication discovery.

Those operations answer different questions:

1. Is the supplied byte slice an exact-end valid current file?
2. Is every linked historical prefix independently valid?
3. Which strictly valid prefixes are discoverable in a bounded suffix after damage or interruption?

Treating those questions as one fallback validator would allow damaged or merely discoverable state to be mistaken for active valid state.

## Decision

The reusable Rust experiment exposes three separate entry points:

- `validate` performs exact-end strict validation only. It never scans for another footer.
- `validate_history` first validates the exact-end commit and then independently revalidates every linked prefix from newest to genesis. It fails closed rather than returning a partial history.
- `scan_recovery_candidates` scans only a caller-bounded suffix and reports strictly validated prefixes from newest to oldest. It never selects a candidate or changes active state.

Recovery footer magic is only a search hint. Every reported prefix must pass the same exact-end strict validator used by `validate`.

History must not trust the newest commit's parent pointer as proof that the parent commit is valid. Every ancestor's own snapshot, commit digest, tree, objects, canonical padding, ranges, and overlaps are rechecked.

## Independent bounds

`ImmutableLimits` carries independent ceilings for:

- exact file bytes, objects, pages, depth, allocation, and output;
- linked history entries;
- recovery suffix bytes;
- recovery footer-validation attempts;
- returned recovery candidates.

Recovery charges every footer magic hit against the attempt cap, including invalid candidates. Candidate and attempt truncation are reported separately.

## Consequences

- A valid newest commit can coexist with invalid historical integrity; `validate` may succeed while `validate_history` fails.
- An interrupted append can yield one or more valid historical prefixes, but recovery does not authorize any of them as active.
- Callers must make recovery-selection and freshness decisions outside the codec.
- No API name or report type implies authenticity, provenance, confidentiality, signer trust, or external freshness.
- The current history implementation revalidates prefixes from slices and may repeat work. A future bounded source implementation may optimize reads but must preserve the same assurance boundaries.

## Validation evidence

The focused Rust suite covers:

- newest-to-oldest two-commit history;
- ancestor commit-digest corruption that leaves current exact-end validation successful but makes history fail;
- history-entry limits;
- exact-end and interrupted-append recovery;
- zero scan windows and zero attempt budgets;
- candidate caps retaining only the newest validated prefix while marking truncation;
- deterministic pinned genesis, append, and multi-level byte identities.

A dedicated cargo-fuzz target generates bounded two-commit files, traverses history, scans recovery candidates, truncates publication, and corrupts an ancestor digest.

## Non-goals

This ADR does not allocate Candidate 2, choose a normative recovery policy, define freshness, add a remote source adapter, implement repair, or promise compatibility for the successor microformat.
