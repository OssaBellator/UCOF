# Experiment 0011: EXP-0002 Page Sequence and Historical Reuse

- **Status:** Reproducible byte-level evidence
- **Date:** 2026-07-30
- **Related:** FCP-0002, ADR-0012, Experiment 0008
- **Script:** `tools/experiment_exp0002_page_sequence_reuse.py`

## Question

Can Candidate 1 reuse an unchanged historical directory page in a later snapshot, or does the page's authenticated snapshot-sequence field require every page to be rewritten?

## Relevant Candidate 1 rules

Each 16 KiB page stores a sequence number at page-header bytes 32 through 39. The page digest authenticates all page bytes. Strict validation also requires every referenced page sequence to equal the active snapshot sequence.

These rules give a page one-snapshot identity unless the sequence has a different meaning than currently specified.

## Method

The experiment uses the independent Python codec to:

1. create a 400-object genesis snapshot with three leaf pages and one internal root;
2. append object 401, producing a sequence-one snapshot;
3. compare old and new pages with and without the eight sequence bytes;
4. identify unchanged leaf ranges whose entries, locators, padding, and all other header fields are identical;
5. construct a forged-but-fully-reauthenticated append that replaces the sequence-one duplicate of leaf range 1–185 with the exact sequence-zero historical page;
6. update the current root child digest, root digest, snapshot digest, footer semantics, and commit digest;
7. run the independent strict validator.

The forged file is not rejected for an outer digest mismatch. Every affected authentication layer is recomputed so validation reaches the page-to-snapshot sequence check.

## Result

The genesis and append each contain four directory pages.

- zero pages are byte-for-byte reusable under Candidate 1;
- at least two leaf pages are identical after masking only the eight sequence bytes;
- ranges 1–185 and 186–370 have unchanged entries and physical object locators;
- a fully reauthenticated reuse of the historical 1–185 leaf is rejected as `page reference mismatch` because its page sequence is zero and the active snapshot sequence is one.

## Finding

**Candidate 1 cannot perform true byte-for-byte historical page reuse under its current page-sequence rule.**

Even an unchanged leaf must be copied and have its sequence rewritten. Rewriting the sequence changes the page digest, which changes every ancestor digest through the root. The existing COW model demonstrates the desired persistent-tree algorithm, but Candidate 1 bytes prevent that algorithm from reusing old page bytes.

This is not a performance-only detail. It determines page identity, digest scope, append amplification, history sharing, and what a directory root authenticates.

## Consequences

The FCP must not state that Candidate 1 merely lacks an implementation of page reuse. The current bytes and validator semantics prohibit it.

A future candidate must select and specify one of these approaches:

1. **Content-addressed immutable pages:** remove active-snapshot sequence binding from page bytes and let the snapshot authenticate the root graph.
2. **Page birth generation:** retain a generation field but validate it as page creation history rather than requiring equality with the active snapshot.
3. **Snapshot-specific pages:** intentionally keep the equality rule and abandon historical page reuse, accepting full-page rewrite amplification.
4. **Separate identity and publication metadata:** move snapshot publication context outside immutable page content.

The first three options have materially different recovery, canonicalization, and privacy properties and require explicit byte-level experiments.

## Security considerations

Removing sequence equality must not permit an attacker to splice an unauthenticated page from another file or snapshot. Root and child digests already authenticate exact page bytes, but a revised design must still define:

- file or epoch domain separation;
- whether page bytes can be shared across file instances;
- whether physical locators are part of page identity;
- how page cycles and conflicting ranges fail closed;
- whether equality of page digests leaks cross-snapshot or cross-file content;
- how repair and compaction report preserved page and snapshot identities.

Keeping sequence binding avoids some replay ambiguity inside one snapshot graph but creates deterministic full-directory rewrite amplification. That trade-off must be evaluated rather than hidden.

## Decision impact

This finding is a blocker for moving FCP-0002 to Review. Candidate 1 may remain a disposable baseline, but a page-reuse claim requires revised bytes or revised validation semantics and new cross-language vectors.

## Reproduction

```console
python3 tools/experiment_exp0002_page_sequence_reuse.py
```

The script asserts zero exact page reuse, at least two sequence-only-equivalent leaves, the expected unchanged ranges, and rejection of the fully reauthenticated historical-page reuse at the sequence check.
