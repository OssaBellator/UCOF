# Experiment 0083: persistent mixed streaming output

## Question

Can the authenticated canonical mixed writer emit a complete successor file through bounded sequential writes without materializing a second full-file output buffer?

## Construction

`append_persistent_mixed_batch_to` validates the exact-end canonical base and applies the same canonical mixed-operation ordering, exact current-page reuse, final locator regrouping, and linked publication semantics as `append_persistent_mixed_batch`.

The new writer constructs only an append tail. New object and page offsets are computed as `base_length + tail_length`, so encoded locator and child references remain byte-identical to the owned writer. The commit digest covers the tail from the exact end of the prior footer through the new snapshot and the same footer semantics. After all validation, limit checks, reuse decisions, tail construction, and hashing finish, the verified base slice and append tail are copied to the sink in bounded requests.

## Evidence

Unit tests cover:

- byte-for-byte equality with the owned canonical mixed writer;
- equal public report, mode, pages written, and pages reused;
- exact base and tail byte accounting;
- tail allocation smaller than the complete successor in a multi-page case;
- strict canonical validation of streamed output;
- invalid-operation failure before output;
- terminal sink failure without a success report.

The `immutable_successor_persistent_mixed_streaming` fuzz target varies bounded root-leaf and multi-leaf bases, delete/replace batches, optional insertion, payload lengths, caller operation order, and sink chunk sizes. It compares the complete streamed bytes and report against the owned writer, validates canonical occupancy, checks exact tail allocation accounting, and confirms caller-order invariance.

## Boundary

This closes whole-successor duplication for canonical mixed batches but does not provide constant-memory tail construction. The implementation still owns the active locator inventory and complete append tail, including new payloads and changed pages. It accepts an in-memory verified base slice rather than a bounded remote source. Sink failure after output begins is terminal; atomic visibility still requires private staging and the qualified publication protocol. Replacement-only, insertion-only, deletion-only, and shared multi-`Put` streaming paths remain separate until they share the same base-offset tail abstraction.
