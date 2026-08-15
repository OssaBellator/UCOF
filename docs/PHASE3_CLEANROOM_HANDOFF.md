# Phase 3 clean-room handoff procedure

This procedure turns the Phase 3 independence requirement into a reproducible package boundary. It is deliberately separate from the in-repository implementation evidence.

A clean-room handoff is useful only after maintainers have made the normative choices clear enough that an independent implementer does not need to read UCOF source code to discover missing rules.

## Readiness gate

Before producing a final handoff:

1. D1–D7 are explicitly selected in `docs/phase3-d1-d7-state.json` with rationale, normative locations, corpus impact and review reference;
2. the coordinated normative amendment has been applied;
3. a fresh EXP-0003 candidate corpus has been generated from those selected rules;
4. the normative text and corpus inputs intended for the independent implementer are identified explicitly.

`tools/build_phase3_cleanroom_handoff.py` refuses final bundle creation while D1–D7 remain unselected.

While the decisions are still open, maintainers may inspect a **non-final manifest plan** with `--allow-unselected-plan --manifest-only`; that mode must not be represented as clean-room readiness.

## Build the handoff

Supply only the normative/public inputs that the external implementer should receive:

```text
python3 tools/build_phase3_cleanroom_handoff.py \
  --input <selected-normative-spec-path> \
  --input <selected-public-vector-or-corpus-path> \
  --output target/phase3-cleanroom-handoff.zip
```

Inputs may be individual files or directories. Paths are normalized to repository-relative names and emitted in sorted order.

The builder rejects:

- `crates/`, `tools/`, `fuzz/`, `.github/` and `target/` inputs;
- Rust, Python, shell, TOML/lock and workflow-source suffixes;
- internal review/experiment/verification documentation;
- symlink inputs;
- paths outside the repository.

This is intentionally strict. If the independent implementer needs implementation source to understand the candidate, the specification is not clean-room ready.

## Deterministic manifest/archive

The bundle contains `CLEANROOM_MANIFEST.json` with:

- bundle schema;
- repository Git SHA when available;
- D1–D7 selection summary;
- an explicit `implementation_source_included: false` claim;
- each bundled file's repository-relative path, exact byte length and SHA-256 digest;
- independent-interpretation instructions.

ZIP member order, timestamps and file modes are fixed so two builds from the same bytes/state are byte-for-byte reproducible.

The builder immediately reopens the generated ZIP and verifies member order, manifest equality, byte lengths and hashes.

## Independent implementer instructions

The external implementer/reviewer should be asked to:

1. interpret only the bundled normative/public material;
2. record every ambiguity or required private clarification as a specification defect;
3. implement the selected candidate independently;
4. regenerate/reproduce the public corpus;
5. explain any byte mismatch in terms of a normative rule rather than copying the in-repo implementation;
6. report rules that permit more than one reasonable byte interpretation even if their chosen interpretation happens to match UCOF.

The independence signal is stronger than “a second program emits the same bytes.” The useful result is that an independently maintained reader can derive the same rules from the text alone.

## Evidence to retain

For each clean-room round, retain outside the generated ZIP:

- exact clean-room bundle SHA-256;
- exact Git SHA used to produce it;
- maintainer review reference for D1–D7;
- external implementation/reviewer identity and repository/revision where appropriate;
- corpus reproduction output;
- ambiguity/clarification log;
- any normative fixes produced by the round;
- a second bundle if normative fixes change the candidate.

Do not silently patch the external implementer's copy. Any clarification needed to make the implementation succeed should become an explicit normative change and a new reproducible handoff.

## What does not count as independent evidence

The following remain useful implementation tests but do not satisfy the clean-room gate by themselves:

- Rust vs Python implementations maintained in the same repository/lineage;
- a wrapper around the Rust implementation;
- an implementation written after reading private implementation code to fill specification gaps;
- corpus byte comparison without an ambiguity review;
- historical GitHub Actions results;
- implementation experiments that have not been promoted through D1–D7 normative selection.

## Current state

As of this packet's creation, D1–D7 remain unselected. Therefore the clean-room builder is **prepared infrastructure**, not evidence that the independent reproduction gate has been satisfied.

No clean-room result should be used to allocate EXP-0003 or claim a stable format without the separate governance decision required by the repository process.
