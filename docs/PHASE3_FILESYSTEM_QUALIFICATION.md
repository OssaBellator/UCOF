# Phase 3 local filesystem qualification boundary

This document separates the local filesystem **mechanism evidence** used by the Phase 3 research implementation from stronger durability claims that still require platform/filesystem qualification.

It is non-normative and does not change FCP-0003, EXP-0003, D1–D7, epoch allocation, or compatibility policy.

## Repository-local smoke harness

Run on the filesystem intended to hold private restart metadata and publication staging:

```text
python3 tools/qualify_phase3_filesystem.py --scratch-dir /path/on/target/filesystem --output target/phase3-filesystem-smoke.json
```

Self-test the harness with:

```text
python3 tools/test_qualify_phase3_filesystem.py
```

The harness records the actual Linux kernel/machine, mount point, filesystem type, mount/superblock options, major/minor device identity, free/available bytes, and inode counts. It then exercises a private temporary subtree with the operation classes used by the research lifecycle:

- private directory mode `0700`;
- private file mode `0600`;
- exclusive file creation;
- complete write followed by file `fsync`;
- directory `fsync`;
- hard-link publication into a separate directory;
- explicit proof that a second link to the same destination fails with `EEXIST` rather than overwriting;
- publication-directory `fsync`;
- private unlink followed by private-directory `fsync` while the published link remains valid;
- publication unlink followed by publication-directory `fsync`.

The harness removes its temporary subtree after the run.

## What a PASS means

A passing report means that, in the observed running environment:

- the relevant syscalls were accepted by the mounted filesystem;
- expected permission and hard-link/no-overwrite behavior was observed;
- file and directory `fsync` calls completed successfully;
- published hard links retained the exact payload after the private pathname was removed;
- current capacity/inode observations were recorded.

That is useful qualification input. It is not a crash/power-cycle experiment.

## What a PASS does **not** mean

The report deliberately sets these claims to false:

- `power_loss_qualified`;
- `anti_rollback_qualified`;
- `same_uid_unlink_race_closed`.

A successful smoke run therefore does not establish:

- persistence across abrupt power loss, controller reset, kernel panic, VM-host failure, or storage-cache loss;
- ordering guarantees stronger than the filesystem/kernel/storage stack documents and an actual crash campaign demonstrates;
- deletion/replay anti-rollback for locally authenticated checkpoint files;
- atomic closure of the final same-UID identity-check-to-unlink race;
- reservation of free blocks or inodes against unrelated concurrent writers;
- forensic secure deletion.

## Network and distributed filesystems

Known network/distributed filesystem types such as NFS/NFS4, CIFS/SMB, Ceph, 9p, SSHFS, GlusterFS, and related mounts are reported as:

```text
unsupported-without-provider-qualification
```

The harness does not assume that local-Linux `fsync`, directory sync, hard-link, cache-coherence, or failure semantics transfer to those providers.

A future provider qualification would need to document the exact service/filesystem, protocol/mount configuration, consistency contract, failure model, and crash/recovery evidence before issue #11 could claim that environment.

## Production qualification still needed

For a specific local filesystem/storage stack, stronger acceptance should pair the mechanical report with a controlled destructive/fault campaign that cuts power or the equivalent at each state transition used by:

- nonce generation record publication;
- compaction checkpoint file sync;
- checkpoint directory sync;
- proof-preserving metadata pruning;
- final prune directory sync;
- encrypted stage manifest publication;
- canonical output publication;
- Prepared retirement publication;
- stage/manifest unlink and directory sync;
- Terminal retirement publication.

The expected recovery classification for every cut should be pinned before the campaign. The campaign should record filesystem, kernel, storage controller/device, cache/barrier configuration, virtualization layer if any, and exact software SHA.

Until that evidence exists, Experiment 0179 and issue #11 should describe the current code as deterministic local-Linux mechanism evidence rather than physical durability qualification.
