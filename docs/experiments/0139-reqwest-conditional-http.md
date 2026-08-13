# Experiment 0139 — Concrete Reqwest conditional HTTP transport

**Status:** non-normative Phase 3 implementation/qualification evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Feature:** `ucof-experiments/http-reqwest`

## Purpose

The immutable-successor research already has transport-neutral policy for:

- one strong source version per assurance operation;
- exact response classification;
- retryable versus terminal failures;
- bounded retry/backoff planning;
- cancellation/deadline control;
- optional explicit authentication refresh.

What it lacked was a maintained HTTP client that actually places those rules on the wire and can abort an in-flight asynchronous request.

This experiment adds an optional concrete transport using Reqwest + Tokio. It is intentionally **single-attempt**: Reqwest's internal request retries are disabled, and retry/backoff authority remains with UCOF's explicit operation-wide policy layer.

This is partial evidence for #10, not issue completion. A versioned cloud-object adapter and end-to-end async assurance integration remain open.

## Dependency choice

The optional feature uses:

```text
reqwest 0.13.4
tokio   1.53.1
```

Reqwest 0.13.4 declares Rust 1.85.0, matching the workspace MSRV gate.

The feature is optional so ordinary workspace builds and the existing cross-target portability checks do not acquire a mandatory network/TLS dependency.

## Client construction policy

`ReqwestConditionalRangeClient` accepts only `http` or `https` URLs and constructs one reusable Reqwest client with:

- redirect following disabled;
- system/environment proxies disabled;
- gzip, Brotli, Zstandard, and deflate auto-decompression disabled;
- automatic Referer disabled;
- Reqwest request retries disabled (`max_retries_per_request(0)`);
- rustls TLS support compiled into the optional feature.

Requests also send:

```text
Accept-Encoding: identity
```

A non-identity `Content-Encoding` response is rejected even if the server ignores the request header.

This makes exact object bytes, redirects, proxy use, decompression, and retry accounting explicit rather than library-default behavior.

## Metadata acquisition

Metadata uses HTTP `HEAD`.

A successful response must classify exactly as the existing generic conditional HTTP policy requires:

```text
status          200
ETag            present and strong
Content-Length  present and valid u64
Content-Range   absent
body            absent by HEAD semantics
```

The resulting exact object length and strong entity tag bind later range requests.

## Conditional range request

A range read emits:

```text
GET <object URL>
Range: bytes=<start>-<end inclusive>
If-Match: <strong ETag>
Accept-Encoding: identity
```

For a successful read, bytes are not returned to the caller until all of these hold:

- status is exactly `206`;
- response ETag equals the expected strong token;
- `Content-Length` equals requested length;
- `Content-Range` start/end/total exactly match request + previously acquired object length;
- body length equals requested length;
- content encoding is absent or `identity`;
- operation cancellation/deadline remains valid after body completion.

HTTP 412 maps to `VersionChanged` before any body bytes can become accepted assurance state.

## Native asynchronous cancellation

The adapter races each Reqwest request/body future against the existing operation control using Tokio `select!`.

The control branch polls the shared atomic cancellation/deadline state every 10 ms. When cancellation or deadline wins, the in-flight Reqwest future is dropped immediately by `select!` rather than waiting for a blocking HTTP call to finish.

This is materially stronger than the existing synchronous cooperative-wait model: it can abort while response headers or a response body are stalled.

The 10 ms control poll interval is research policy, not a stable API promise.

## Local adversarial tests

The feature tests run a real Tokio TCP listener and exercise actual HTTP bytes.

### Exact metadata + range

The server returns:

```text
HEAD -> 200 + ETag "v1" + Content-Length 6
GET  -> 206 + ETag "v1" + bytes 1-3/6 + body "bcd"
```

The test records requests and verifies the actual wire request contains `Range`, `If-Match`, and `Accept-Encoding: identity`.

### Version mutation

A `412 Precondition Failed` range response returns `VersionChanged`.

### Malformed range metadata

A 206 response with a contradictory `Content-Range` is rejected as protocol failure even when body length and ETag otherwise look valid.

### Redirect

A 307 response is observed and rejected. Reqwest does not follow its `Location` target.

### Cancellation during body

The server sends valid 206 headers and only the first byte of a four-byte body, then stalls. Cancelling the operation causes the in-flight body future to return `Cancelled` under a bounded test timeout.

### Deadline during headers

The server accepts the request but sends no response headers. A monotonic operation deadline aborts the in-flight request with `DeadlineExceeded`.

## Intentionally not claimed yet

This experiment does not yet provide:

- an async equivalent of the full `ImmutableReadAt` validation stack;
- operation-wide async retry/backoff execution using the existing planners;
- application-owned async credential refresh;
- one qualified versioned cloud-object provider adapter;
- provider-specific version-token semantics;
- TLS trust-store/certificate-policy qualification beyond Reqwest/rustls defaults;
- proxy support (the current adapter intentionally disables it);
- HTTP-date `Retry-After` parsing (integer delta-seconds only);
- end-to-end targeted lookup/full validation/history/recovery through the async transport;
- a production compatibility or security certification claim.

Those remain follow-up work under #10.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest conditional_reqwest
```

Ordinary workspace/MSRV/portability jobs continue to run without enabling this optional feature. A dedicated native CI step enables it and runs the concrete HTTP tests.
