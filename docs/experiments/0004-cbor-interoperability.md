# Experiment 0004 — Restricted canonical CBOR interoperability

**Status:** Passing Phase 1 comparison  
**Test:** `crates/ucof-core/tests/cbor_interop.rs`  
**Oracle:** `ciborium 0.2.2`, pinned as a development dependency

## Question

Does the UCOF restricted metadata encoder produce established CBOR encodings for the primitive values it supports, and where does its validity model intentionally differ from a general CBOR implementation?

## Results

The UCOF encoder matches Ciborium byte-for-byte for:

- unsigned integers at every encoding-width boundary through `u64::MAX`;
- byte strings;
- UTF-8 text strings;
- booleans and null;
- definite-length arrays;
- maps when the external value is supplied in RFC 8949 deterministic key order.

The comparison also confirms a deliberate difference: Ciborium, as a general CBOR implementation, accepts non-shortest integer encodings and indefinite-length arrays that the UCOF restricted decoder rejects as non-canonical.

## Interpretation

Using CBOR does not by itself guarantee deterministic bytes. UCOF must continue to specify and test:

- shortest argument encodings;
- definite lengths;
- deterministic map-key order;
- duplicate-key rejection;
- the exact supported type subset;
- rejection of trailing values and unsupported major types.

The external library is an interoperability oracle for common encodings, not the normative definition of UCOF metadata. A future version change in the library cannot silently change UCOF validity.

## Decision for EXP-0001

The restricted CBOR subset is sufficiently reproduced for continued experimentation. It remains provisional and must be described directly in the specification and conformance vectors rather than by reference to a Rust crate.
