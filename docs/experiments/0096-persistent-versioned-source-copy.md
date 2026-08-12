# Experiment 0096: Strong-version persistent source copying

## Question

Can the bounded persistent source copier require one strong non-ABA source version across every length and range operation, so bytes from a changed range are rejected before they reach the sink?

## Construction

`PersistentVersionedReadAt` extends the immutable random-access source contract with an opaque 32-byte version token. Equal tokens are required to identify the same immutable object bytes and length for one operation.

`append_versioned_source_with_tail_to`:

1. captures the expected source version;
2. delegates whole-file length, SHA-256 preflight, cumulative budgets, bounded reads, bounded writes, and tail handling to Experiment 0095;
3. checks the expected version immediately before and after every source length or range operation;
4. refuses to expose bytes from a range whose version changed;
5. classifies version changes as preflight or copy phase according to whether sink output has begun;
6. reports the pinned version and total version checks.

## Evidence

Rust tests cover:

- stable exact `base || tail` output;
- preflight version change with an untouched sink;
- copy-phase version change after one previously verified base chunk, with the changed range and tail withheld;
- distinct version-service failure reporting.

The `immutable_successor_persistent_versioned_source_copy` fuzz target varies arbitrary base and tail bytes, source and sink chunk sizes, and token-change positions. Stable versions must reproduce exact output. Preflight changes must write nothing. Copy-phase changes may leave only an exact prefix of the original base and must never expose the changed range or append the tail.

## Boundary

This model depends on provider adapters supplying a genuinely strong non-ABA token. It is not a provider implementation and does not establish HTTP, cloud SDK, or filesystem semantics. Earlier verified base chunks may remain in the sink when a later source operation changes, so atomic visibility still requires private staging and durable no-overwrite publication.
