# Phase 3 live S3 versioned-source qualification

The in-repository S3-shaped source adapter uses S3 `VersionId` as immutable provider-view authority. Deterministic adapter tests are not enough to qualify real provider behavior, credentials or operational policy.

`tools/qualify_phase3_s3_versioned_source.py` provides a bounded live-provider harness using an already-configured AWS CLI.

## Safety boundary

The harness:

- never installs AWS tooling or credentials;
- never changes bucket versioning, IAM, STS, TLS, proxy or lifecycle policy;
- never enumerates/deletes the bucket broadly;
- creates one cryptographically unique test key beneath an explicit qualification prefix;
- deletes only exact `VersionId` / delete-marker identities whose `Key` exactly equals that unique test key;
- requires explicit `--allow-write` before any object mutation;
- records command evidence without credential-bearing argv values;
- treats incomplete cleanup as a failed qualification result unless `--keep-objects` was explicitly requested.

Use a dedicated qualification bucket/prefix and least-privilege credentials where possible. Do not point the harness at a production namespace merely because exact-key cleanup is implemented.

## Read-only readiness observation

Without `--allow-write`, the harness can verify only that the bucket reports versioning `Enabled` and optionally record STS caller identity:

```text
python3 tools/qualify_phase3_s3_versioned_source.py \
  --bucket <qualification-bucket> \
  --region <region> \
  --output target/phase3-s3-versioned-source.json
```

This intentionally returns a non-qualified/readiness result because immutable historical behavior has not been exercised.

## Live write qualification

Run against a versioned qualification bucket:

```text
python3 tools/qualify_phase3_s3_versioned_source.py \
  --bucket <qualification-bucket> \
  --region <region> \
  --prefix ucof-phase3-qualification/ \
  --payload-bytes 1048576 \
  --allow-write \
  --output target/phase3-s3-versioned-source.json
```

The harness then verifies:

1. bucket versioning is `Enabled`;
2. a first put returns a non-empty `VersionId`;
3. putting the **same payload again** returns a different `VersionId`, demonstrating that payload equality/ETag is not the immutable provider identity;
4. a third, different payload produces another distinct `VersionId`;
5. reading the first explicit `VersionId` after later writes reproduces its exact SHA-256 payload bytes;
6. an unversioned current read returns the latest payload;
7. a ranged read against the historical `VersionId` returns the exact requested historical prefix;
8. `head-object --version-id` succeeds for the historical version;
9. a fabricated/nonexistent `VersionId` is rejected;
10. creating a delete marker for the unique test key does not make the historical explicit `VersionId` unreadable;
11. cleanup removes only exact versions/delete markers for that unique key and verifies none remain.

The report records the observed `VersionId` values, ETags as provider metadata, payload lengths/hashes, command result summaries and cleanup evidence. ETag equality is recorded but **not required** and is never treated as the strong immutable identity.

## Permission evidence

The live operations naturally test the credentials actually supplied to the AWS CLI:

- `GetBucketVersioning`;
- `PutObject`;
- `GetObject` / ranged `GetObject`;
- `GetObjectVersion` through explicit `--version-id` reads;
- `HeadObject` / explicit historical head;
- `DeleteObject` and exact version deletion for cleanup;
- `ListBucketVersions` for exact-key cleanup.

A permission failure is useful qualification evidence for that identity, but this harness does not mutate IAM policy to explore a permission matrix.

A separate operator/provider review should still state the intended least-privilege policy and distinguish ordinary current-object permissions from explicit version permissions.

## Credentials / STS boundary

The harness may record `get-caller-identity` account/ARN when available, but does not claim:

- STS refresh/expiry behavior under a long-running writer;
- role assumption/chaining policy;
- credential source precedence;
- metadata-service hardening;
- secret storage/rotation.

Those remain explicit provider/application qualification items.

## TLS / proxy boundary

The harness uses the AWS CLI's configured HTTPS transport and therefore exercises the operator's current network path, but it does **not** qualify:

- minimum TLS version/cipher policy;
- custom CA trust policy;
- forward/transparent proxy semantics;
- TLS interception policy;
- DNS pinning/resolution behavior;
- retry behavior under network faults.

Retain the relevant AWS CLI/network configuration separately for any production qualification claim.

## Scale boundary

`--payload-bytes` permits repeatable larger-object measurements, but a successful single run is not provider-scale qualification. Production evidence should additionally cover the intended object-size/count/concurrency envelope and request/latency/retry limits.

## Evidence retention

For a provider qualification round retain:

- exact UCOF Git SHA;
- harness JSON report;
- AWS CLI version;
- bucket region and versioning status;
- caller identity/role reference where policy allows;
- intended IAM policy reviewed separately;
- TLS/proxy/network policy reference;
- payload-size/concurrency profile;
- cleanup result;
- any provider errors and their request IDs when present.

Do not use a successful live-provider report to select D1–D7, allocate EXP-0003 or claim stable-format compatibility. This is provider qualification for the remote-source mechanism only.
