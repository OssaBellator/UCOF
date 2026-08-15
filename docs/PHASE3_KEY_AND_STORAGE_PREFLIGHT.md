# Phase 3 key-material and storage preflight boundary

This document covers deployment-adjacent checks that can be performed locally without GitHub Actions:

- static key-file hygiene;
- deterministic additional-private-inode planning;
- point-in-time filesystem byte/inode headroom.

All are non-normative implementation/qualification aids. They do not change FCP-0003, EXP-0003, D1–D7, epoch allocation, or compatibility policy.

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

The checker opens each key read-only with close-on-exec and no-follow semantics where the platform provides them, then validates the opened descriptor rather than trusting only pathname metadata. It also validates the resolved immediate parent directory because a private file inside a writable parent is not a meaningful pathname-hygiene boundary.

It requires:

- exactly 32 bytes;
- regular file;
- effective-UID ownership;
- exactly one hard link;
- owner-read permission;
- no group/world permission bits on the key file;
- an immediate parent directory owned by the effective UID;
- owner-execute permission on that parent;
- no group/world write bits on that parent;
- distinct AES and HMAC inodes;
- distinct AES and HMAC secret bytes.

The JSON report contains only metadata such as path, mode, uid/gid, device/inode, link count, width, and immediate parent-directory metadata. It deliberately does **not** print key bytes, hashes, fingerprints, derived key IDs, or any other reusable secret-derived value.

### Key preflight non-claims

A PASS does not establish secure key generation/provisioning, rotation/revocation, KMS/HSM/TPM backing, memory locking, zeroization, backup/recovery policy, anti-rollback, multi-process distribution safety, or descriptor-pinned validation of every ancestor pathname component.

The immediate-parent check closes the obvious writable-parent replacement hole in the local file preflight; it is **not** a general proof against ancestor-path replacement or privileged/same-UID interference after preflight.

## Private inode lifecycle planner

The byte lifecycle planner is not an inode planner: bytes, directory entries, open file descriptors and filesystem inodes are different resources. Use:

```text
python3 tools/plan_phase3_private_inodes.py \
  --max-initial-runs <bounded-spill-max-initial-runs> \
  --output target/phase3-private-inodes.json
```

The v1 inode planner reports **additional free inodes required above the files already present when the operation starts**. It derives its normal-path peak from actual file overlap:

- fresh nonce generation record;
- encrypted descriptor destination stage;
- bounded spill runs, including the one merge-output inode created before its inputs are removed;
- durable restart stage + manifest + source-set authority;
- retained/locator/page-reference tree working stages;
- one staged canonical output inode.

It separately models compacted crash-resume/retirement and conservatively allows the fresh nonce, staged output, Prepared, Terminal and new checkpoint to coexist before reclamation credits are taken.

The resulting `required_additional_inodes` is suitable as the minimum `--required-inodes` input to the storage headroom observer. Publication by hard link contributes **zero additional inodes** because the destination name references the same staged-output inode; it still consumes a directory entry.

The planner deliberately does not claim inode reservation or protection against unrelated concurrent inode consumption.

## Storage headroom observation

Use the deterministic lifecycle byte requirement and inode planner result as inputs to:

```text
python3 tools/check_phase3_storage_headroom.py \
  --path /path/on/private-filesystem \
  --required-bytes <planner-required-bytes> \
  --required-inodes <private-inode-plan-required-additional-inodes> \
  --reserve-bytes <deployment-safety-margin> \
  --reserve-inodes <deployment-safety-margin> \
  --output target/phase3-storage-headroom.json
```

The checker reads `statvfs` and compares current **available** bytes/inodes with `required + deployment reserve`, with byte and inode dimensions reported independently. Its report always records:

```text
reserved = false
race_free = false
```

A PASS is only a point-in-time admission observation. Another process can consume blocks or inodes immediately afterward. It does not replace filesystem/provider reservation, quota mechanisms, deployment isolation, or policy for delayed allocation, copy-on-write, metadata overhead, snapshots, sparse files or provider semantics.

## Recommended local deployment bundle

`tools/verify_phase3_deployment_preflight.py` now derives the inode requirement itself. Supply the exact byte-plan result and the same bounded-spill configuration used by the candidate:

```text
python3 tools/verify_phase3_deployment_preflight.py \
  --filesystem-path /path/on/private-filesystem \
  --aes-key /secure/path/aes.key \
  --hmac-key /secure/path/hmac.key \
  --required-bytes <planner-required-bytes> \
  --max-initial-runs <bounded-spill-max-initial-runs> \
  --reserve-bytes <deployment-safety-margin> \
  --reserve-inodes <deployment-safety-margin> \
  --output target/phase3-deployment-preflight.json
```

The bundle schema is `ucof-phase3-deployment-preflight-v3`. It embeds the private inode plan and passes its derived requirement to the storage child. `--required-inodes` remains available only as an **operator floor**: it can raise the requirement, never lower the lifecycle planner result. The bundle validates that the storage child actually used the effective inode requirement.

Before treating an environment as a serious Phase 3 candidate, retain for the same revision:

1. deterministic code acceptance from `tools/verify_phase3_local.py --acceptance`;
2. SHA-bound acceptance record from `tools/record_phase3_local_acceptance.py`;
3. filesystem mechanical/qualification report from `tools/verify_phase3_qualification_local.py`;
4. key-material preflight for the actual local key files if file-backed secrets are used;
5. the private inode plan and storage headroom observation;
6. separate destructive/power-loss evidence before making physical-durability claims.

No combination of these local preflights selects EXP-0003 wire decisions or turns the current research mechanism into a stable compatibility contract.
