# Experiment 0075: active-file streaming rewrite

## Question

Can the strictly validated active state of a successor file be reissued through the canonical streaming sink without cloning active payloads, copying inactive historical records, or buffering the complete output?

## Adapter

`active_file_payload_sources` first performs complete canonical exact-end validation. It then converts each active authenticated locator into one borrowed payload source:

- object identifier and kind come from the validated leaf locator;
- the payload range is derived from the validated object record offset, record length, and logical length;
- the object digest is used as the immutable source version;
- exact reads remain bounded to the validated payload range;
- no payload bytes are cloned into the adapter inventory.

`rewrite_active_file_to` passes those borrowed sources to the source-backed canonical sink from Experiment 0073.

## Evidence

Pinned cases cover:

- a 400-object file with one historical replacement, producing output byte-for-byte equal to `rewrite_all` while using 31-byte payload reads and 113-byte sink writes;
- source and output report equality with the existing slice-based rewrite;
- tampered source rejection before any sink byte is written;
- a replaced 4,096-byte historical payload that is not reread, while only the 12-byte active replacement and the other 17-byte active payload are streamed;
- fuzzed active/slice rewrite equivalence, optional historical replacement, bounded requests, canonical validity, and tamper-before-output rejection.

## Boundary

The adapter accepts an in-memory file slice. It does not yet expose a bounded remote-range inventory, selected historical snapshot streaming, or semantic-compaction streaming. Locator inventory remains proportional to active object count. Source or sink failure after output begins still requires private staging for atomic visibility.
