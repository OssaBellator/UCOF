# Local Phase 3 verification

GitHub Actions is not the acceptance authority for the current Phase 3 / Experiment 0179 work. Use the repository-local verifier from a clean checkout of the exact candidate commit.

This document is operational guidance only. It does not change FCP-0003, EXP-0003, D1–D7, epoch allocation, or compatibility policy.

## Fast development gate

Run:

```text
python3 tools/verify_phase3_local.py
```

This checks the Experiment 0179 wiring and fail-closed guard tokens, runs the independent restart-metadata compaction model, runs the Phase 3 Python tool self-tests, and executes the fast Phase 3 Rust/model/corpus surface. Because the qualification-helper self-tests exercise POSIX ownership and filesystem-capacity APIs, this gate requires a POSIX host; use `--model-only` for the portable second-model check.

When dependencies are already cached and network access should be forbidden:

```text
python3 tools/verify_phase3_local.py --offline
```

The run writes:

```text
target/phase3-local-verification.json
```

A fast development report is useful for iteration but is **not** acceptance evidence.

## Independent model only

For a cheap second-model check without Cargo:

```text
python3 tools/verify_phase3_local.py --model-only
```

Or run the model directly with a larger campaign:

```text
python3 tools/verify_restart_metadata_compaction_model.py --campaigns 1024 --steps 256
```

The model is intentionally independent of the Rust implementation. A passing model cannot promote Experiment 0179 by itself.

## Full local acceptance

Start from a clean worktree at the exact candidate SHA and run:

```text
python3 tools/verify_phase3_local.py --acceptance
```

The verifier refuses a dirty worktree before expensive commands begin. It records that initial SHA as `acceptance_sha`, then verifies again after the complete acceptance surface that `HEAD` is unchanged and the worktree is still clean. Child Python checks run with bytecode-cache writes disabled so the verifier does not dirty the candidate checkout itself. It never installs missing packages/toolchains.

A complete acceptance environment must already provide:

- a POSIX host exposing `os.geteuid` and `os.statvfs` for the filesystem/key/storage qualification helpers;
- the repository's normal Rust toolchain and locked dependencies;
- Rust 1.85.0;
- `i686-unknown-linux-gnu`;
- `powerpc64-unknown-linux-gnu`;
- nightly Rust;
- `cargo-fuzz` and any local linker/runtime support needed by the portability/fuzz checks.

The acceptance run includes:

- static 0179 wiring/stale-API/fail-closed guards, including checkpoint-history consistency, bounded transient checkpoint headroom and destructive prune ordering;
- independent restart-metadata compaction model;
- Phase 3 Python tool self-tests for the verifier, recorder, filesystem/key/storage/deployment qualification helpers;
- locked Cargo metadata;
- Rust formatting;
- Phase 3 and workspace Clippy with `-D warnings`;
- Phase 3 and workspace Rust tests plus documentation tests;
- existing independent Phase 3/corpus/vector checks;
- Reqwest conditional HTTP and versioned S3 adapter tests;
- deletion-policy examples/vector checks;
- Rust 1.85 workspace and HTTP-feature checks;
- i686 and powerpc64 checks;
- fuzz target formatting/build/list plus a 256-run smoke pass for every listed fuzz target.

`--skip-fuzz` exists only as a development escape hatch. If it is used, the final report is intentionally not acceptable.

`--model-only` and `--acceptance` are mutually exclusive so a report cannot ambiguously claim both modes.

## Inspect the report

A candidate acceptance report must contain all of:

```text
schema = ucof-phase3-local-verification-v1
mode = acceptance
ok = true
dirty_worktree = false
skipped = []
acceptance_sha = <exact candidate commit at start>
git_sha = <same exact candidate commit at completion>
```

Every check record must have `status = pass`. Historical GitHub Actions results are not substitutes for this report.

## Record successful acceptance

After the full verifier succeeds, while still on the same clean Git SHA, run:

```text
python3 tools/record_phase3_local_acceptance.py
```

The recorder verifies:

- report schema/mode/result;
- `acceptance_sha == git_sha == current HEAD`;
- current worktree is still clean;
- no skipped checks;
- Python/Rust/Cargo version evidence is present;
- the complete required check set is present and passing, including the Phase 3 Python tool self-tests;
- `cargo fuzz list` produced a non-empty, duplicate-free target set;
- every listed fuzz target has a matching passing smoke record and no unexpected smoke target exists.

It hashes the exact source report bytes with SHA-256 and writes a normalized evidence record:

```text
docs/verification/phase3-local-acceptance-<40-char-sha>.json
```

The record schema is `ucof-phase3-local-acceptance-v2`; it contains the accepted SHA, branch, source-report digest, tool versions, fuzz-target list, normalized checks and explicit non-claims.

Commit that evidence record in a follow-up evidence commit. Re-running the recorder against an identical existing record is idempotent; different evidence for the same SHA is rejected.

Because creating the evidence JSON changes the worktree, the verification SHA named by the record is the **candidate code SHA immediately before the evidence-only commit**. The evidence commit must not be treated as a newly verified code SHA unless the verifier is run again on it.

## Promotion rule for Experiment 0179

Do not change `docs/experiments/0179-restart-metadata-compaction.md` from pending to accepted until a SHA-bound local acceptance record exists for the exact candidate code commit and its report is complete.

Even then, acceptance means repository implementation evidence only. It does not qualify physical power-loss behavior, filesystem semantics, production key management, local anti-rollback, same-UID unlink-race closure, free-space reservation, or any EXP-0003 wire/governance decision.
