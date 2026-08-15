# Phase 3 destructive power-loss campaign

The deterministic fault models and `tools/qualify_phase3_process_crash_cuts.py` provide strong restart/syscall evidence, but they do not prove storage-image survival after kernel/host/controller/power failure.

`tools/plan_phase3_powerloss_campaign.py` defines the external destructive campaign required before making a physical-durability claim for the Phase 3 checkpoint/publication/retirement mechanisms.

This campaign is intentionally **not** executed automatically by the repository local acceptance gate.

## Why a separate destructive campaign is required

A normal process crash leaves the kernel, page cache, filesystem, block layer and storage controller alive. `fsync`/directory-sync ordering can therefore be exercised, but not the exact storage image that would result from:

- sudden host power loss;
- VM/hypervisor reset;
- kernel panic;
- block-device/controller reset;
- cloud-volume host failure;
- write-cache loss.

The destructive campaign must reboot or remount from the actual post-cut storage image.

## Canonical case plan

Inspect the complete plan with:

```text
python3 tools/plan_phase3_powerloss_campaign.py \
  > target/phase3-powerloss-plan.json
```

The v1 plan contains 14 cases covering:

- 0179 checkpoint creation and every destructive-prune dependency boundary;
- staged/public hard-link publication boundaries;
- Prepared/cleanup/Terminal retirement boundaries.

Each case specifies:

- precondition;
- exact cut location;
- durable operations completed before the cut;
- operations deliberately not completed;
- required reboot observation;
- required retry behavior;
- safety invariant that must hold.

Do not silently omit a difficult case. The final validator accepts only the complete canonical case order.

## Initialize a result file

Create a deliberately incomplete template:

```text
python3 tools/init_phase3_powerloss_results.py \
  target/phase3-powerloss-results.json
```

The generated result uses `status: "unexecuted"` for every case and blank platform/evidence fields. It is **supposed** to fail final validation until the campaign is complete.

## Required platform identity

One result file must describe one coherent storage stack. Record at least:

- UCOF Git SHA;
- kernel version;
- filesystem type and version where available;
- exact mount options;
- block-device/cloud-volume type;
- storage controller or virtualization layer;
- write-cache policy where known;
- host/cloud provider;
- test image/snapshot identifier.

If any of these materially change, start a new result file rather than combining cases from different environments.

## Recommended VM/block-image procedure

A VM or disposable host with snapshot-capable storage is the safest repeatable setup.

For each case:

1. provision or restore a known-clean baseline image;
2. install/build the exact accepted UCOF candidate and the cut-driving harness for that case;
3. record the image/snapshot identifier and platform metadata;
4. drive the workload to the exact case precondition;
5. arm the cut immediately after the listed durable operations and before the listed incomplete operations;
6. trigger a destructive reset/power cut that prevents graceful shutdown and subsequent syncs;
7. boot or attach **the resulting storage image**, without replacing it with a clean snapshot;
8. capture the reboot-visible directory entries, file bytes/identities and authenticated authority state before retrying;
9. run the specified restart/recovery action from a fresh process/boot context;
10. record the retry outcome and preserve logs/images needed to reproduce the observation;
11. restore the clean baseline before the next case.

The mechanism used to trigger the cut must itself be documented by `cut_execution_reference`. Examples include a hypervisor hard-reset script, external PDU/power controller, cloud instance force-stop primitive, or storage-controller fault injection. A normal `kill -9`/`os._exit()` does not count as the destructive cut.

## Case result requirements

Each case must be marked exactly `pass` or `fail`. `skipped`, `unknown`, `not-applicable` and missing cases are intentionally rejected.

For every case fill:

- `cut_execution_reference` — where the exact destructive cut and trigger evidence are retained;
- `reboot_observation` — what existed/authenticated immediately after reboot before recovery changed state;
- `retry_result` — what the prescribed restart/recovery action did;
- optional `notes` for provider/kernel/filesystem observations.

A `fail` is a valid, reviewable experimental result but causes the overall validator to return nonzero. Do not rewrite a failed case as skipped.

## Validate the completed result

```text
python3 tools/verify_phase3_powerloss_results.py \
  target/phase3-powerloss-results.json
```

The validator rejects:

- wrong plan/result schema;
- missing platform identity;
- missing/reordered/duplicate cases;
- skipped or unknown status values;
- missing cut/reboot/retry evidence;
- incomplete campaign metadata.

It returns success only when all 14 canonical cases are present and explicitly pass.

## Evidence retention

Retain the validated JSON together with:

- the exact plan JSON;
- machine/VM image or snapshot IDs;
- cut-controller logs and timestamps;
- console/kernel/filesystem recovery logs;
- before/after directory/object inventories;
- UCOF restart/recovery output;
- storage-provider incident/request IDs if relevant;
- the exact accepted UCOF Git SHA and local acceptance record;
- operator identity and campaign start/completion timestamps.

Where feasible, retain immutable hashes for large image/log artifacts in the campaign evidence location rather than embedding them in the result JSON.

## Interpretation of a passing campaign

A complete passing campaign qualifies only the **identified storage stack and tested failure mechanism**. It does not automatically transfer to another filesystem, mount option, kernel, cloud volume, storage controller or provider.

Network/distributed filesystems need their own provider-specific qualification because a local block-filesystem power-cut campaign does not model remote durability acknowledgements.

## Explicit non-claims

A passing destructive campaign still does not by itself establish:

- D1–D7 normative selection or EXP-0003 allocation;
- production key provisioning/rotation;
- deletion/replay anti-rollback without external freshness authority;
- same-UID final check-to-unlink race closure;
- byte/inode reservation against concurrent consumers;
- provider/IAM/TLS policy for remote immutable sources;
- forensic secure deletion.

Physical-durability qualification is one production-readiness dimension, not a format-governance decision.
