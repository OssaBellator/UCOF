# Issue #11 production-qualification ledger

**Scope:** non-normative Phase 3 implementation/review ledger  
**Tracking:** issue #11 — encrypted spill and qualified durable publication  
**Wire/governance:** no EXP-0003 D1–D7 selection; no epoch allocation; no compatibility promise

This ledger separates **implemented research mechanisms** from **production-qualified claims**. A successful local implementation test or deployment-adjacent preflight can move an evidence row without automatically moving its platform/operational qualification row.

Status vocabulary:

- **evidence** — executable repository evidence exists;
- **partial** — important implementation evidence exists, but the stated production claim still has an explicit qualification gap;
- **open** — no sufficient evidence for the production claim;
- **external** — requires provider/platform/operator evidence that cannot be established by in-repository deterministic tests alone.

## Confidential private stages

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| bounded encrypted descriptor spill | Experiments 0170–0172, consolidated through 0177 | evidence | production key lifecycle; platform/provider crypto qualification |
| encrypted retained descriptors | 0170/0177 spine | evidence | same |
| encrypted locator stages | 0177 | evidence | same |
| encrypted page-reference stages | 0177 | evidence | same |
| deterministic public bytes despite randomized private ciphertext | 0171–0172/0177 regressions | evidence | broader workload/operational qualification |
| clear sorter key/object-id confidentiality | explicitly not provided | open | redesign if this confidentiality property is required |
| spill geometry confidentiality | explicitly not provided | open | policy/redesign if required |

## Nonce and restart authority

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| no nonce issuance before durable lease reservation | 0157, 0174 | evidence | physical filesystem qualification |
| burned committed ranges are not reused | 0174, 0177; 0179 independent model + wired retry regression pending local Rust acceptance | partial | complete pinned local acceptance; physical filesystem qualification |
| authenticated restart metadata | 0173–0175 | evidence | production HMAC key lifecycle |
| exact external source resource set/order binding | 0178 opaque caller-owned `source_set_id` | evidence | provider/application derivation policy |
| restart after metadata checkpoint compaction | 0179 independent model passes; wired Rust pending | partial | clean `tools/verify_phase3_local.py --acceptance` report on pinned head; platform qualification |
| every surviving checkpoint/history pair is mutually consistent | 0179 all-checkpoint scan rule + masked-history regressions wired | partial | complete pinned local Rust acceptance |
| local anti-rollback against deletion/replay | HMAC/checkpoint explicitly insufficient | open | non-rollbackable external anchor/trusted floor |
| production key provisioning and rotation | none claimed | open | key-management integration and failure policy |

## Private-storage/resource bounds

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| bounded sorter runs/fan-in/descriptors/live spill | 013x–0171 | evidence | production operating limits/profile |
| one normal/crash-resume private lifecycle arithmetic cap | 0176 | evidence | filesystem reservation/concurrent-consumption policy |
| encrypted tree widths included in cap | 0177 | evidence | same |
| source-set authority included in cap | 0178 | evidence | same |
| checkpoint/protected metadata included in cap | 0179 independent model + wired quota regressions pending local Rust acceptance | partial | complete pinned local acceptance; full lifecycle integration decision |
| checkpoint transient directory-entry headroom | exact-cap, unknown-entry rejection, current-checkpoint-only lending, `usize::MAX` saturation and compacted-commit reservation regressions are wired | partial | complete pinned local Rust acceptance |
| compacted publication/Prepared metadata headroom | two-slot pre-publication reservation + exact/one-short regressions + post-publication Prepared recheck | partial | complete pinned local Rust acceptance; concurrency/isolation policy outside the checked pinned directory |
| append-only source-set authority write capacity | shared journal metadata-slot guard enforced immediately before source-set `create_new`; exact-cap regression wired | partial | complete pinned local Rust acceptance |
| ordinary stage-manifest / ordinary retirement primitive write capacity | high-level lifecycle admission exists in relevant paths, but no shared primitive guard is claimed for every ordinary writer | open | decide whether all append-only metadata primitives require the shared slot guard and add exact-cap regressions if so |
| deterministic additional private inode demand | `plan_phase3_private_inodes.py` exact-boundary tests; deployment preflight v3 derives requirement from `max_initial_runs` | evidence | observation/admission is not reservation; filesystem/operator policy still required |
| point-in-time byte/inode headroom observation | `check_phase3_storage_headroom.py` exact-boundary tests and deployment bundle validation | evidence | observation is not reservation; operator/filesystem policy still required |
| actual free-space reservation | arithmetic/`statvfs` observation only | open | platform mechanism or explicit fail-on-consumption policy |
| inode reservation/competition | deterministic demand + observations but no reservation | open | filesystem/operator policy |

## Key material / deployment preflight

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| local file-backed AES/HMAC width/ownership/permission hygiene | `qualify_phase3_key_material.py`; exact 32-byte, regular, effective-UID-owned, single-link, no group/world file permissions | evidence | does not qualify generation/provisioning/rotation/storage policy |
| immediate key-parent replacement hygiene | preflight requires effective-UID-owned parent, owner execute, and no group/world write bits; dedicated regression | evidence | ancestor path is not descriptor-pinned; privileged/same-UID replacement remains outside this preflight |
| AES and HMAC are distinct local files and bytes | key preflight checks inode identity and byte inequality without emitting secret-derived fingerprints | evidence | operational key separation/provisioning policy |
| deployment bundle child evidence integrity | `verify_phase3_deployment_preflight.py` v3 validates filesystem/key/storage schemas and derives inode demand instead of trusting exit codes/operator undercounts | evidence | bundle remains deployment-adjacent, not production acceptance |
| production key provisioning / rotation / revocation | explicitly not claimed | open | KMS/HSM/file provisioning design, rotation and failure behavior |
| key-memory locking / zeroization / hardware backing | explicitly not claimed | open | platform/key-management mechanism if required |

## Publication authority

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| private validation before publication | 015x/0175 | evidence | physical filesystem qualification |
| no-overwrite same-filesystem publication | Linux descriptor-pinned backend + 0175 | evidence | supported-filesystem qualification |
| parent directory sync before `PublishedAndDurable` | 0175 | evidence | physical power-loss qualification |
| destination-exists preserves prior destination | regressions, including compacted burn/prune/retry path | evidence | supported-filesystem qualification |
| link/parent-sync indeterminate outcome is explicit | 0175 | evidence | operator recovery procedure |
| cross-filesystem publication | intentionally not equivalent | partial | explicit weaker/refusal production policy |
| network filesystem/NFS semantics | smoke harness classifies network/distributed mounts as unsupported without provider evidence | external | supported/unsupported matrix with real provider qualification |

## Cleanup/retirement

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| no destructive retirement before durable public publication | 0175 | evidence | physical filesystem qualification |
| Prepared before unlink; Terminal after directory sync | 0175 | evidence | physical filesystem qualification |
| both cleanup targets classified before first unlink | 0175 | evidence | same-UID race remains |
| restart after unlink crash cuts | 0175 + process-crash harness/tool self-tests | partial | execute retained process-crash campaign on candidate environment; physical power-loss remains separate |
| append-only metadata reclamation | 0179 implementation + independent state model, local Rust acceptance pending | partial | complete pinned local acceptance; retention/cadence policy |
| dependency-safe compaction prune order | nonce → terminal source-set → Prepared → Terminal → old checkpoint, plus source/Prepared crash-prefix retry models/tests | partial | complete pinned local Rust acceptance |
| final identity-check -> unlink same-UID race | documented non-claim | open | stronger platform primitive or explicit isolation assumption |
| forensic secure deletion | explicitly not claimed | open/non-goal | separate policy/mechanism if ever required |

## Remote immutable-source / provider qualification

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| versioned strong-source S3-shaped adapter | implementation on main/Phase 3 spine | evidence | live-provider qualification |
| qualification harness safety/reproducibility | `qualify_phase3_s3_versioned_source.py` + fake-CLI self-tests, acceptance-loaded as tooling tests | evidence | harness self-tests do not constitute live AWS/S3 evidence |
| live versioning-enabled S3 version identity/range behavior | qualification procedure/tool exists | external | execute against identified bucket/region/provider and retain report |
| IAM/version-specific permissions and denied-operation behavior | qualification procedure/tool exists | external | execute least-privilege/negative cases in real provider context |
| STS/role expiry and credential refresh | not established by local deterministic tests | external | real credential-lifecycle campaign |
| TLS trust-store / enterprise proxy policy | explicit provider/deployment concern | external | supported environment policy + evidence |
| provider-scale latency/request/byte measurements | not established locally | external | measured provider campaign under intended operating limits |

## Clean-room / governance-adjacent evidence

These rows do not close issue #11 directly, but they prevent production-qualification work from being mistaken for normative convergence.

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| explicit D1–D7 decision state | `docs/phase3-d1-d7-state.json` + fail-closed validator/self-tests; current state remains unselected | evidence | maintainer decisions and review references |
| clean-room handoff construction | `build_phase3_cleanroom_handoff.py` + self-tests | evidence | independent recipient must perform interpretation before inspecting implementation internals |
| independent corpus byte comparison | `compare_phase3_cleanroom_corpus.py` + self-tests; comparator does not execute UCOF implementation code | evidence | actual independently generated corpus + ambiguity report |
| independent implementation/external review | tooling/handoff exists only | external | satisfy issue #12 with real independent evidence |

## Fault injection / adversarial behavior

| Requirement | Current evidence | Status | Remaining qualification |
|---|---|---:|---|
| ciphertext corruption/truncation/substitution | AEAD/HMAC tests across 0170–0178; 0179 authenticated-graph cases wired | partial | complete pinned local 0179 acceptance; broader campaign optional |
| nonce uniqueness/no-wrap | lease model + restart tests; 0179 independent repeated-compaction campaigns and exhaustion boundary regressions | partial | complete pinned local 0179 acceptance; long platform campaign |
| directory path replacement | procfd descriptor-pinning evidence | evidence | same-UID final-step race remains |
| staged-name replacement | detected at classification/final checks in publication/retirement paths; 0179 re-authenticates selected metadata immediately before prune | partial | complete pinned local 0179 acceptance; final-step race remains |
| symlink staging directory | rejected | evidence | supported-platform qualification |
| short write / ENOSPC / sync failures | abstract/publication fault models exist | partial | concrete Linux/filesystem fault-injection matrix |
| process crash at durability transitions | `qualify_phase3_process_crash_cuts.py` + self-tests provide a fresh-process harness | partial | execute and retain candidate/environment report; process crash is not power loss |
| destructive storage-image power loss | canonical 14-case `plan_phase3_powerloss_campaign.py`, template and strict result validator exist | external | run every case on each claimed storage stack and retain cut/reboot/retry evidence |
| concurrent unrelated disk consumption | arithmetic quota and `statvfs` preflight cannot prevent | open | reservation/operator policy |

## Local verification authority

GitHub Actions is not the acceptance authority for the current 0179 work. Earlier #141 workflow results are specifically **not** 0179 evidence because the first 0179 files were not wired into the Rust test module.

The repository-local replacement is:

```text
python3 tools/verify_phase3_local.py --acceptance
```

A complete acceptance report is `target/phase3-local-verification.json` with `mode: "acceptance"`, `ok: true`, no skipped checks, and `acceptance_sha == git_sha` for the exact candidate. The runner pins a clean SHA before expensive work and rechecks HEAD/worktree cleanliness after the full gate.

The gate covers static 0179 wiring/fail-closed guards, the independent compaction model, Phase 3 Python tool self-tests, Rust fmt/Clippy/tests, workspace/doc tests, HTTP/S3 and policy/vector gates, Rust 1.85, i686, powerpc64, and local fuzz smoke for every target reported by `cargo fuzz list`. The Phase 3 Python tool aggregate now also exercises the fail-closed D1–D7 state validator, clean-room handoff/comparator, S3 harness self-tests, process-crash harness self-tests, power-loss plan/template/result validators, journal-entry lifecycle model, and journal-capacity static guards. It does **not** execute live AWS operations or destructive power cuts.

`tools/record_phase3_local_acceptance.py` additionally requires the same clean current SHA, hashes the exact source report, verifies the complete check set and exact fuzz-target coverage, and writes a normalized `ucof-phase3-local-acceptance-v2` record.

`--model-only`, `--skip-fuzz`, historical Actions results, or a report from another/moved SHA cannot promote Experiment 0179.

## Platform qualification still required

These claims remain **external/open** even if Experiment 0179 receives a complete local acceptance report:

1. complete destructive 14-case power-loss qualification of file/directory durability ordering on each supported storage stack;
2. explicit supported local-filesystem matrix and network-filesystem refusal/qualification policy;
3. production key provisioning/rotation/storage and failure behavior;
4. non-rollbackable freshness authority if rollback resistance is claimed;
5. stronger same-UID isolation or an explicit deployment assumption for the final check -> unlink race;
6. free-space/inode competition policy beyond arithmetic/preflight observation;
7. live S3/provider/IAM/STS/TLS qualification for remote immutable sources;
8. qualification outside the native Linux x86_64 AWS-LC experiment.

## Closure rule for issue #11

Issue #11 should not close merely because all deterministic mechanism rows become `evidence`.

Closure requires the repository to state exactly which platform/filesystem/key-management/isolation assumptions are production-supported, attach reproducible evidence for those claims, and keep all stronger non-claims explicit. Any unsupported environment must fail closed or be documented as a weaker policy mode rather than inheriting Linux research claims by implication.
