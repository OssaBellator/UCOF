# Phase 3 key-material and storage preflight boundary

This document covers two deployment-adjacent checks that can be performed locally without GitHub Actions:

- static key-file hygiene;
- point-in-time filesystem byte/inode headroom.

Both are non-normative implementation/qualification aids. They do not change FCP-0003, EXP-0003, D1–D7, epoch allocation, or compatibility policy.

## Key-file preflight

For a deployment that currently supplies the research implementation with local 32-byte AES and HMAC files, run:

```text
python3 tools/qualify_phase3_key_material.py \
  --aes-key /secure/path/aes.key \
  --hmac-key /secure/path/hmac.key \
  --output target/phase3-key-material-preflight.json
```

Self-test the checker with:

```text
python3 tools/test_qualify_phase3_key_material.py
```

The checker opens each file read-only with close-on-exec and no-follow semantics where the platform provides them, then validates the opened descriptor rather than trusting only pathname metadata.

It requires:

- exactly 32 bytes;
- regular file;
- effective-UID ownership;
- exactly one hard link;
- owner-read permission;
- no group/world permission bits;
- distinct AES and HMAC inodes;
- distinct AES and HMAC secret bytes.

The JSON report contains only metadata such as path, mode, uid/gid, device/inode, link count, width, and parent-directory metadata. It deliberately does **not** print key bytes, hashes, fingerprints, derived key IDs, or any other reusable secret-derived value.

### Key preflight non-claims

A PASS does not establish:

- secure key generation or provisioning;
- key rotation/revocation semantics;
- KMS/HSM/TPM backing;
- secret memory locking;
- zeroization after use;
- backup/recovery policy;
- anti-rollback or freshness anchoring;
- multi-process key distribution safety.

Those remain production design/qualification work under issue #11.

## Storage headroom observation

Use the deterministic lifecycle planner's required byte count as input to:

```text
python3 tools/check_phase3_storage_headroom.py \
  --path /path/on/private-filesystem \
  --required-bytes <planner-required-bytes> \
  --required-inodes <expected-new-inodes> \
  --reserve-bytes <deployment-safety-margin> \
  --reserve-inodes <deployment-safety-margin> \
  --output target/phase3-storage-headroom.json
```

Self-test with:

```text
python3 tools/test_check_phase3_storage_headroom.py
```

The checker reads `statvfs` and compares current **available** bytes/inodes with:

```text
required + deployment reserve
```

It uses exact arithmetic and reports byte/inode headroom independently, including one-byte/one-inode boundary behavior.

### Storage preflight non-claims

The report always records:

```text
reserved = false
race_free = false
```

A PASS is only a point-in-time admission observation. Another process can consume blocks or inodes immediately afterward. This helper therefore complements, but does not replace:

- deterministic private-lifecycle quota accounting;
- filesystem/provider-specific reservation or quota mechanisms;
- operational isolation from unrelated writers;
- handling of delayed allocation, copy-on-write, metadata overhead, snapshots, quotas, sparse files, or provider billing/storage semantics.

## Recommended local qualification bundle

Before treating a deployment environment as a serious Phase 3 candidate, retain all of the following for the same software/environment revision:

1. successful deterministic code acceptance from `tools/verify_phase3_local.py --acceptance`;
2. SHA-bound acceptance record from `tools/record_phase3_local_acceptance.py`;
3. filesystem mechanical/qualification report from `tools/verify_phase3_qualification_local.py`;
4. key-material preflight report for the actual local key files if file-backed secrets are used;
5. storage headroom observation using the exact lifecycle planner requirement;
6. separate destructive/power-loss evidence before making physical-durability claims.

No combination of these local preflights selects EXP-0003 wire decisions or turns the current research mechanism into a stable compatibility contract.
