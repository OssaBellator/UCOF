# Phase 3 process-crash durability qualification

`tools/qualify_phase3_process_crash_cuts.py` exercises selected Phase 3 durability transitions against a real Linux filesystem using fresh child processes that terminate with `os._exit()` at exact cuts.

This evidence sits between deterministic unit/fault models and destructive machine/power-loss testing.

It is **not** a physical power-loss test.

## What the harness exercises

The harness creates one unique scratch subtree containing private, publication and journal directories. It then runs four cases.

### 1. Checkpoint file sync → crash before directory sync

The child:

1. creates the checkpoint with create-new semantics;
2. writes exact bytes;
3. `fsync`s the file;
4. exits immediately before syncing the containing directory.

The fresh parent verifies that the visible checkpoint has exact bytes and then re-syncs the directory before treating it as retry authority.

This mirrors the 0179 rule that a visible matching checkpoint after the file-sync cut must still pass directory re-verification/sync before destructive pruning.

### 2. Publication hard link → crash before publication-directory sync

The staged output is file-synced and its private directory synced first. The child then hard-links it to the final publication name and exits before syncing the publication directory.

The fresh parent verifies:

- both names are visible;
- both names identify the same device/inode;
- payload bytes match;
- publication-directory sync can be completed on retry;
- repeating the no-overwrite link fails with `EEXIST` and does not replace the prior destination.

This exercises the observable process-crash state behind the `PublishedAndDurable` boundary without claiming that the pre-directory-sync link would survive sudden power loss.

### 3. Retirement crash after stage unlink

Both cleanup targets are created and durably staged first. The child removes the private restart stage and exits before removing the manifest or syncing the directories.

The fresh parent observes the partial state, removes the still-present manifest and syncs both directories.

### 4. Retirement crash after both unlinks, before directory sync

The child removes both cleanup targets and exits before syncing either directory. The fresh parent verifies both names absent and performs the directory syncs required before terminal cleanup authority could be written.

## Run locally

Use a dedicated scratch path on the filesystem being evaluated:

```text
python3 tools/qualify_phase3_process_crash_cuts.py \
  --scratch-dir /path/on/candidate-filesystem \
  --output target/phase3-process-crash-cuts.json
```

The tool creates one UUID-scoped directory beneath the supplied scratch directory and removes it when complete.

Self-tests:

```text
python3 -m unittest tools.test_qualify_phase3_process_crash_cuts
```

## Evidence value

A passing report demonstrates that, for the actual kernel/filesystem/mount in that run:

- the expected process-crash-visible states can be classified;
- retry `fsync` operations can be completed in a fresh process;
- same-filesystem no-overwrite hard-link publication has the expected inode semantics;
- cleanup retries tolerate already-absent targets in the modeled order.

This is useful evidence for restart logic and syscall assumptions.

## Explicit non-claims

Every report records these as false:

- `physical_power_loss_simulated`;
- `kernel_page_cache_dropped`;
- `storage_controller_cache_flushed_by_power_cut`;
- `filesystem_crash_consistency_proven`;
- `network_filesystem_qualified`;
- `same_uid_unlink_race_closed`;
- `production_accepted`.

A process exit leaves the kernel, page cache and storage stack running. Therefore it cannot prove that the same pre-sync state would survive a kernel panic, hypervisor reset, host crash, controller reset or sudden loss of power.

## Stronger qualification still required

A production physical-durability claim should additionally use a destructive harness appropriate to the target environment, for example VM/block-device snapshot or host power-cycle testing that can cut execution after each asserted file/directory sync boundary and then boot/recover from the resulting storage image.

That campaign must identify the exact filesystem, mount options, kernel, storage/controller/cache configuration and virtualization/storage provider. Network/distributed filesystems require separate provider-specific evidence rather than inheriting local-filesystem results.

This qualification remains independent of D1–D7 and EXP-0003 byte selection.
