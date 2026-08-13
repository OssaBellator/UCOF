# Experiment 0146 — Versioned S3 strong-source adapter

**Status:** non-normative Phase 3 provider-shaped implementation evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Depends on:** Experiments 0142–0145

## Purpose

Experiments 0142–0145 establish the complete UCOF assurance-operation set over a generic strong-version HTTP source. This experiment adds one provider-shaped cloud-object adapter for Amazon S3 versioned objects without making S3 behavior part of UCOF bytes.

The design intentionally uses S3 object `versionId` as the immutable source version. ETag is not treated as the provider version authority.

## Version identity

S3 returns an opaque `x-amz-version-id` for versioned objects and accepts `versionId` on version-specific object requests.

`S3VersionedReqwestClient` therefore:

1. sends one authenticated HEAD for the current object;
2. requires an exact content length and a non-empty, non-`null` `x-amz-version-id`;
3. losslessly wraps the opaque version-id header bytes in the generic UCOF `StrongVersionToken` as a quoted `s3v1:` hex token;
4. decodes that token back to the exact provider version ID for every later range request;
5. sends every later GET with that exact `versionId` query parameter;
6. requires the returned `x-amz-version-id` to match the selected version before accepting body bytes.

This gives the UCOF source layer an immutable provider object version rather than relying on equality of mutable-object ETags.

## SigV4 request binding

The adapter implements AWS Signature Version 4 using the existing SHA-256 dependency rather than adding an AWS SDK or another cryptographic crate.

For version-specific range GETs the canonical request binds:

```text
method
canonical object path
versionId query
host
Range
x-amz-content-sha256
x-amz-date
optional x-amz-security-token
```

The GET payload hash is the SHA-256 of the empty request body.

The implementation includes a deterministic check against AWS's published 24 May 2013 S3 `GET /test.txt` `Range: bytes=0-9` SigV4 example. The resulting signature must equal:

```text
f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41
```

This checks HMAC derivation, canonical header ordering, signed-header selection, payload hash, scope, and final authorization formatting against an external provider reference rather than only self-consistency.

## Credential handling

`S3SigV4Credentials` contains:

- access-key ID;
- secret access key;
- optional temporary-session token.

Debug output redacts all credential values. Session-token header values are marked sensitive before entering Reqwest.

This experiment uses caller-supplied static credentials. It does not implement an AWS credential-provider chain, instance-role refresh, STS refresh, or workload-identity integration.

## Endpoint and HTTP policy

Production construction defaults to the explicit `HttpsOnly` policy.

`AllowHttpEmulator` exists only for loopback/provider-emulator qualification where test PKI would add unrelated complexity.

As with the generic Reqwest adapter:

- redirects are disabled;
- system proxies are disabled;
- automatic gzip/Brotli/Zstd/deflate decoding is disabled;
- automatic Referer behavior is disabled;
- Reqwest internal retries are disabled;
- UCOF owns the explicit operation-wide transport-attempt/backoff policy.

## Response validation

HEAD requires:

- HTTP 200;
- no unsupported content encoding;
- exact `Content-Length`;
- non-null `x-amz-version-id`;
- no current delete-marker response.

Version-specific GET requires:

- HTTP 206;
- returned version ID exactly equal to the selected provider version;
- exact `Content-Length`;
- exact `Content-Range` start/end/full-object length;
- exact body length;
- no unsupported content encoding.

Retryable HTTP status handling remains explicit and bounded. Authentication/version/protocol failures are not silently retried as valid source state.

## Provider-shaped emulator coverage

A loopback S3-compatible test server checks that:

- HEAD obtains a version ID and exact length;
- every later GET contains the percent-encoded exact `versionId` query;
- every range GET has a SigV4 Authorization header whose signed-header set includes `Range`;
- `Accept-Encoding: identity` is retained;
- provider `Content-Range` totals remain the complete immutable object length;
- targeted lookup executes through the S3 adapter;
- strict full validation executes through the S3 adapter;
- linked-history validation executes through the S3 adapter;
- report-only recovery executes through a versioned S3-shaped object.

## Assurance boundary

This is provider-shaped implementation and deterministic signing evidence. It does **not** by itself establish live Amazon S3 interoperability or production credential/TLS qualification.

Still outside this experiment:

- live AWS account qualification against a versioning-enabled bucket;
- IAM-policy and version-specific permission matrix;
- STS/role credential refresh and expiry behavior;
- TLS trust-store/certificate/proxy enterprise qualification;
- S3 endpoint variants such as access points, Outposts, directory buckets, or arbitrary custom path-style endpoints;
- provider-scale latency, retry, request-count, and byte-cost measurements;
- freshness or authorization that the selected object version is the newest legitimate state.

Those remain implementation/qualification work under #10. They are not EXP-0003 wire fields.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest s3_versioned_reqwest
```

The HTTP feature must remain compatible with Rust 1.85.0. Default i686 and powerpc64 portability checks remain required.

## Governance boundary

This adapter operates over current immutable-successor research bytes and the generic strong-version source contract. It does not select D1–D7, change FCP-0003 status, allocate EXP-0003, or make Amazon S3 a mandatory UCOF transport.
