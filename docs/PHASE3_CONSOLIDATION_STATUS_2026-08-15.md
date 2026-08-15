# Phase 3 consolidation status — 2026-08-15

**Scope:** non-normative implementation/convergence status  
**Branch:** `phase-3/restart-metadata-compaction` / PR #141 stack  
**Wire/governance:** FCP-0003 remains Draft; no D1–D7 selection; no EXP-0003 epoch allocation; no compatibility promise

This brief exists because implementation evidence has moved faster than several older status documents. It does not replace normative governance or a pinned acceptance record.

## Executive position

Phase 3 is no longer blocked primarily on discovering the basic implementation architecture. The repository now has substantial executable evidence for bounded construction, authenticated persistent state, remote immutable sources, encrypted private stages, crash-aware restart, durable publication, retirement, and metadata compaction.

The remaining work has separated into three different classes and should be managed separately:

1. **implementation acceptance/consolidation** — finish and locally accept Experiment 0179 on the exact wired Rust head;
2. **platform/provider qualification** — power-loss/filesystem, key-management, resource-reservation and live-provider evidence that deterministic repository tests cannot prove;
3. **normative convergence** — D1–D7 maintainer decisions, coordinated EXP-0003 amendment/corpus, independent clean-room interpretation and later allocation decision.

Treating all three as one undifferentiated “Phase 3 TODO” now creates more confusion than progress.

## Accepted private-writer implementation spine

The current consolidation spine is:

- **0176** — unified encrypted private-storage lifecycle accounting across normal and restart/publication/retirement paths;
- **0177** — encrypted locator/page-reference tree stages consolidated onto the durability/restart line with one combined fresh nonce lease;
- **0178** — authenticated caller/provider-owned opaque source-set identity bound to the exact durable restart stage before a fresh continuation lease is minted.

These are implementation/research evidence only. They do not select EXP-0003 wire policy.

Historical branches before this spine remain useful evidence but should not be treated as parallel production implementations. In particular, duplicate sorter/HMAC lines should remain historical unless a specific invariant is missing from the consolidation spine.

## Experiment 0179 — implemented frontier, acceptance pending

Experiment 0179 is the current integration frontier. Its implementation now covers:

- authenticated 112-byte nonce checkpoints as replacement recovery bases;
- checkpoint file sync + pinned-directory sync before destructive pruning;
- retry of a visible file-synced checkpoint with directory re-verification/re-sync before prune;
- every-checkpoint/surviving-record consistency, preventing a newer checkpoint from masking an older contradiction;
- embedded-generation/filename replay rejection for ordinary nonce records;
- monotonic finite/exhausted nonce authority and trusted-floor checks;
- authenticated manifest/source-set/retirement graph validation;
- bounded directory-inventory work rather than loops over attacker-influenced numeric generation ranges;
- exact preservation of historical nonce records only while a live encrypted stage needs them for lease verification;
- compacted retry from historical stage authority while allocation uses current global nonce authority;
- `gen1 stage -> burn/prune gen2 -> retry/publish gen3 -> Prepared -> Terminal -> final reclaim`;
- `DestinationExists` burn -> checkpoint/prune -> later strictly monotonic retry;
- dependency-safe destructive order: `eligible nonce -> terminal source-set -> Prepared -> Terminal -> old checkpoint`;
- immediate re-authentication/recomparison of selected metadata immediately before unlink;
- checkpoint/protected metadata included in private-byte admission;
- continuation and durable publication one-byte-short rejection before nonce/backend side effects;
- current-checkpoint-only transient `max_directory_entries + 1` scan headroom;
- unknown-entry rejection when borrowing the transient checkpoint slot;
- saturating `usize::MAX` defensive scan-ceiling arithmetic;
- compacted nonce commit admission that preserves room for a future checkpoint;
- compacted durable publication admission that preserves **two journal slots** before fresh nonce issuance: fresh nonce + Prepared cleanup authority;
- Prepared retirement recheck immediately before persistence;
- low-level configured-cap enforcement for ordinary nonce record creation;
- deterministic additional-private-inode planning for spill/restart/tree/output/checkpoint overlap.

The source fixes for stale-checkpoint transient headroom and `usize::MAX` overflow are implemented and wired with regressions. They are no longer design blockers.

### Why 0179 is still pending

The pending reason is now **acceptance execution**, not a known missing directory-headroom design fix.

The first #141 drafts were not wired into the Rust compilation graph, so historical GitHub Actions success is invalid as 0179 evidence. GitHub Actions is also no longer the current acceptance path.

0179 may move from pending only when the exact candidate SHA passes:

```text
python3 tools/verify_phase3_local.py --acceptance
python3 tools/record_phase3_local_acceptance.py
```

with a clean unchanged candidate SHA, no skipped checks, complete Python/static guards, Rust fmt/Clippy/tests, workspace/docs, HTTP/S3/policy/vector checks, Rust 1.85, i686, powerpc64, and smoke execution for every target returned by `cargo fuzz list`.

The current development environment should not be used to promote 0179 unless that complete Rust/toolchain surface is actually available and succeeds.

## Resource admission is now two-dimensional

Phase 3 already had a deterministic private-byte lifecycle cap. The branch now also contains:

```text
tools/plan_phase3_private_inodes.py
```

which derives **additional free inode demand above operation-entry inventory** from the bounded-spill `max_initial_runs` configuration plus fixed restart/tree/publication/retirement overlap.

`tools/verify_phase3_deployment_preflight.py` v3 requires `--max-initial-runs`, derives this inode requirement, and passes it to the point-in-time `statvfs` headroom observer. Any operator-specified inode requirement acts only as a higher floor.

This closes a planning gap; it does **not** reserve blocks or inodes against other processes.

## Local deployment-adjacent evidence

The branch also has reproducible helpers for evidence that belongs outside deterministic wire/algorithm acceptance:

- filesystem mechanics/mount classification;
- independent Terminal-last prune-order campaign;
- local file-backed AES/HMAC hygiene;
- immediate key-parent ownership/write-permission hygiene;
- byte/inode headroom observation;
- deployment bundle child-report schema/evidence validation.

These helpers deliberately preserve non-claims for:

- physical power-loss persistence;
- network/distributed filesystem equivalence;
- local deletion/replay anti-rollback;
- final Linux same-UID check-to-unlink race closure;
- free-space/inode reservation;
- production key generation/provisioning/rotation/HSM/KMS policy;
- ancestor pathname pinning for file-backed keys.

They are qualification inputs, not substitutes for deterministic acceptance or production policy.

## Remote immutable-source track

The strong-version remote-source implementation track is architecturally mature: authenticated lookup/absence, full validation, linked-history verification, recovery/reporting, and a versioned S3-shaped adapter are part of the Phase 3 evidence base.

The remaining value on that track is primarily **real-provider qualification** rather than another in-memory abstraction: versioned S3 behavior, IAM/version permissions, STS credential lifecycle, TLS/proxy policy, provider-scale limits and reproducible live-provider evidence.

Do not reopen already-demonstrated remote-source architecture unless provider evidence exposes a concrete missing invariant.

## Normative convergence remains independent

No amount of green 0179 implementation evidence selects D1–D7. The normative convergence sequence remains:

1. maintainers explicitly select D1–D7;
2. apply one coordinated normative amendment rather than piecemeal drift;
3. regenerate a fresh EXP-0003 candidate corpus from the selected byte rules;
4. advance FCP-0003 from Draft only through the repository's governance process;
5. obtain meaningful external/clean-room interpretation and reproduction;
6. make a separate experimental epoch/allocation decision.

The outstanding decision categories remain:

- **D1** — Candidate 1 disposition;
- **D2** — ObjectId / geometry choices;
- **D3** — occupancy and split rules;
- **D4** — deletion policy;
- **D5** — catalog semantics;
- **D6** — hash/domain/kind choices;
- **D7** — determinism boundary.

Implementation experiments can constrain these choices, but they must not silently make them.

## Recommended execution order

The highest-leverage Phase 3 order from this point is:

1. keep #141 stable enough to obtain a **real pinned local Rust acceptance report** for 0179;
2. if acceptance exposes defects, fix only those defects and rerun the same gate rather than opening another parallel experiment line;
3. record the accepted SHA-bound 0179 evidence and update the private-writer stack/index;
4. run/retain deployment-adjacent filesystem/key/inode evidence for candidate environments without promoting those observations to production claims;
5. finish live-provider qualification for the remote-source track;
6. move maintainer attention to D1–D7 and the coordinated EXP-0003 amendment/corpus;
7. commission independent/clean-room reproduction before any allocation/stability claim.

## Explicit non-claims

This status does not claim:

- Experiment 0179 is accepted;
- issue #11 is production-qualified or ready to close;
- physical durability has been demonstrated;
- any network/distributed filesystem is supported;
- key lifecycle is production-qualified;
- resource admission is reservation;
- rollback resistance exists without an external trusted freshness floor;
- EXP-0003 byte rules are selected;
- a stable format or UCOF 1.0 compatibility contract exists.
