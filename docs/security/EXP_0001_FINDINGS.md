# UCOF-EXP-0001 Security Findings

**Status:** Phase 1 threat-model evidence  
**Related:** `docs/THREAT_MODEL.md`, `docs/proposals/0001-exp-0001-framing.md`

## Confirmed controls

The Rust and independent Python implementations now demonstrate:

- file-size rejection before structural parsing;
- bounded record, payload, metadata, text, byte-string, depth, and item limits;
- checked range validation for footer, directory, record, and payload locations;
- fail-closed handling of unknown required capabilities;
- rejection of non-shortest, indefinite, duplicate-key, out-of-order, invalid UTF-8, negative-integer, and floating-point CBOR forms in the restricted metadata subset;
- directory cross-checking against physical framing rather than trusting index metadata;
- exact-end footer discovery;
- digest verification after structural framing checks and before semantic directory or manifest use;
- rejection at every truncated byte boundary in the Rust demonstration corpus;
- independent agreement on the shared valid and malformed vector corpus.

## Findings from mutation tests

Validation order affects the observed error category. A mutation of a framing length is rejected structurally before digest comparison, while a payload mutation that preserves framing reaches `digest_mismatch`. Tests must therefore target a specific validation layer rather than assume every changed committed byte produces the same error.

Footer fields are outside the committed-prefix digest in EXP-0001. They require explicit structural and semantic validation. Changing the manifest identifier, record count, directory range, or digest produces distinct failures without first invalidating the prefix digest.

Changing a committed record identifier and recomputing the experimental digest reaches directory cross-validation and is rejected as `directory_mismatch`. This confirms that the directory is not authoritative.

## Footer-search finding

A bounded 64 KiB backward search can be populated with thousands of fake footer magic values. Candidate validation must therefore have explicit scan-byte and candidate-count limits. Normal validation should not silently fall back from exact-end discovery to recovery search.

## Residual risks and follow-up

- SHA-256 in EXP-0001 detects changes relative to the stored footer but provides no authenticity or rollback protection.
- The in-memory Rust API does not yet demonstrate bounded streaming or range-source behavior.
- The metadata subset still needs differential comparison with an established deterministic-CBOR implementation.
- Very large UC-02 and UC-10 workloads need generated scale tests rather than extrapolation from small fixtures.
- Parser fuzzing and sanitizer-backed native dependency review begin in Phase 2.
- Recovery and active-root selection remain intentionally absent until Phase 3 defines strict, checkpoint, and salvage modes separately.

These findings supplement the initial threat model. They must be folded into the normative threat analysis before FCP-0001 can be accepted or a later epoch promoted.
