# Experiment 0045 — Spill ownership, cleanup, and durability policy

## Status

Executable non-normative successor writer-security evidence.

## Question

Can a spill-backed writer make ownership, cleanup authority, resource ceilings, publication durability, and confidentiality limits explicit enough to fail closed under hostile filesystem state?

## Model

`tools/experiment_exp0002_spill_security_policy.py` uses real temporary filesystem objects to exercise:

- private `0700` staging directories and `0600` files;
- exclusive no-follow creation where the platform exposes `O_NOFOLLOW`;
- cumulative byte, inode, and simultaneously-open descriptor limits;
- a caller-held random ownership token stored in a single-link regular marker;
- cleanup that refuses symlinks, non-regular entries, hard-linked entries, and ownership mismatch;
- file synchronization before publication and parent-directory synchronization after publication;
- no-overwrite publication using a hard-link operation whose destination creation is atomic on the tested filesystem.

The model deliberately separates publication visibility from a universal durability guarantee. A production implementation must probe whether its target filesystem and operating system provide the required atomic-link/rename and directory-sync semantics.

## Cases

The executable cases prove:

1. staging objects are private to the process owner;
2. byte, inode, and descriptor limits fail before creating another run;
3. an existing destination is never overwritten;
4. a malicious symlink causes cleanup to stop without touching its external target;
5. an ownership-token mismatch prevents cleanup;
6. only verified single-link regular files in the owned workspace are removed;
7. file and parent-directory sync calls occur in publication order;
8. the policy makes no secure-deletion claim.

## Security findings

- **Cleanup authority is separate from pathname possession.** A process must retain an unpredictable ownership token and validate the marker before removing spill state.
- **Scavenging must be symlink-safe and hard-link-aware.** Recursive convenience APIs that follow links are not acceptable for hostile or shared storage.
- **All three storage resources matter.** Byte limits alone do not prevent inode or descriptor exhaustion.
- **No-overwrite is mandatory.** Existing output must never be replaced as a side effect of publication.
- **Durability is capability-dependent.** File sync, directory sync, atomic visibility, and crash semantics vary by platform and filesystem and must be probed or configured.
- **Unlink is not secure deletion.** SSD remapping, copy-on-write filesystems, snapshots, backups, and remote storage make confidentiality-grade erasure a separate storage-layer concern.

## Non-claims

This experiment does not prove all filesystems implement hard-link publication, durable directory sync, or equivalent Windows primitives. It does not define one portable API, encrypt spill data, or guarantee secure deletion. Production profiles must select supported primitives and fail closed when their required durability or confidentiality properties are unavailable.

## Reproduction

```text
python3 tools/experiment_exp0002_spill_security_policy.py
```
