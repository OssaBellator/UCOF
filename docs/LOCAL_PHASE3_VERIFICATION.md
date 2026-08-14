# Local Phase 3 verification

GitHub Actions is not the acceptance authority for the current Phase 3 / Experiment 0179 work. Use the repository-local verifier from a clean checkout of the exact candidate commit.

This document is operational guidance only. It does not change FCP-0003, EXP-0003, D1–D7, epoch allocation, or compatibility policy.

## Fast development gate

Run:

```text
python3 tools/verify_phase3_local.py
```

This checks the Experiment 0179 wiring, runs the independent restart-metadata compaction model, and executes the fast Phase 3 Rust/model/corpus surface.

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

The verifier refuses a dirty worktree before expensive commands begin. It never installs missing packages/toolchains. A complete acceptance environment must already provide:

- the repository's normal Rust toolchain and locked dependencies;
- Rust 1.85.0;
- `i686-unknown-linux-gnu`;
- `powerpc64-unknown-linux-gnu`;
- nightly Rust;
- `cargo-fuzz` and any local linker/runtime support needed by the portability/fuzz checks.

The acceptance run includes:

- static 0179 wiring/stale-API guard;
- independent restart-metadata compaction model;
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

## Inspect the report

A candidate acceptance report must contain all of:

```text
schema = ucof-phase3-local-verification-v1
mode = acceptance
ok = true
dirty_worktree = false
skipped = []
git_sha = <exact candidate commit>
```

Every check record must have `status = pass`. Historical GitHub Actions results are not substitutes for this report.

## Record successful acceptance

After the full verifier succeeds, while still on the same clean Git SHA, run:

```text
python3 tools/record_phase3_local_acceptance.py
```

The recorder verifies:

- report schema/mode/result;
- exact report SHA equals current `HEAD`;
- current worktree is still clean;
- no skipped checks;
- the complete required check set is present and passing;
- every fuzz target reported by `cargo fuzz list` has a passing smoke record.

It then writes:

```text
docs/verification/phase3-local-acceptance-<40-char-sha>.json
```

Commit that evidence record in a follow-up evidence commit. Re-running the recorder against an identical existing record is idempotent; different evidence for the same SHA is rejected.

Because creating the evidence JSON changes the worktree, the verification SHA named by the record is the **candidate code SHA immediately before the evidence-only commit**. The evidence commit must not be treated as a newly verified code SHA unless the verifier is run again on it.

## Promotion rule for Experiment 0179

Do not change `docs/experiments/0179-restart-metadata-compaction.md` from pending to accepted until a SHA-bound local acceptance record exists for the exact candidate code commit and its report is complete.

Even then, acceptance means repository implementation evidence only. It does not qualify physical power-loss behavior, filesystem semantics, production key management, local anti-rollback, same-UID unlink-race closure, free-space reservation, or any EXP-0003 wire/governance decision.
