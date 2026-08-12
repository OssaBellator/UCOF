# Phase 3 Status — Directory, Snapshots, Recovery, and Successor Convergence

**Status:** In progress; implementation consolidated on `main`, normative successor convergence remains open  
**Started:** 2026-07-30  
**Authoritative implementation baseline:** `main` after PR #75 (`27956368289b6ebf230ff2baf5fa5313d4f83806`)  
**Current convergence milestone:** #76 — EXP-0003 Interoperability Candidate  
**Convergence plan:** `docs/EXP_0003_INTEROP_PLAN.md`

## Current objective

Phase 3 is no longer primarily an implementation-volume phase.

The repository now contains substantial executable evidence for bounded random access, immutable primary-directory pages, append-only snapshots, persistent copy-on-write mutation, linked history, explicit recovery, rewrite/compaction inputs, source-backed operation planning, transport policy, and staged publication research.

The remaining Phase 3 objective is to turn that research into **one internally consistent, independently implementable experimental successor epoch** without mistaking green reference code for normative specification acceptance.

Until that convergence is complete, Phase 4 transform/compression work should not establish new byte-level dependencies on unresolved successor layout decisions.

## Post-consolidation repository state

PR #75 consolidated the Phase 1–3 research implementation and the active green Phase 3 frontier into `main`. The former deep stacked-PR topology is no longer the authoritative implementation structure.

Older Phase 3 pull requests, handoffs, experiments, and status snapshots remain useful historical evidence, but statements that identify PR #3 or another stacked branch as the current implementation baseline are historical rather than current.

The current authority split is:

- **`main`** — authoritative repository implementation baseline;
- **open FCP/policy PRs and issues** — unresolved normative decisions;
- **open evidence/research PRs** — independent measurements or review boundaries that are not implementation dependencies;
- **EXP-0001 / EXP-0002 Candidate 1** — disposable historical experimental evidence with no compatibility promise.

## Epoch boundaries

### UCOF-EXP-0001

EXP-0001 remains the disposable minimal framing experiment and Phase 2 safety-first codec evidence. It demonstrates deterministic framing, bounded hostile-input handling, streaming and random-access reader/writer APIs, independent corpus checking, and explicit assurance boundaries.

It is not a promotion candidate and has no stable compatibility promise.

### UCOF-EXP-0002 Candidate 1

Candidate 1 remains executable historical evidence for authenticated paged directories, snapshots, exact-end publication, bounded source access, history, recovery, repair, and rewrite.

Candidate 1 is rejected as the reusable-page successor baseline because its page identity binds the active snapshot sequence into page authentication, preventing byte-for-byte reuse of unchanged historical pages. It should be retained as negative/security evidence rather than silently upgraded into the next epoch.

Candidate 1 remains unpublished and has no compatibility promise.

### Immutable-page successor research

The current research successor removes active snapshot sequence from immutable page identity and uses content-addressed pages. It is the implementation direction currently represented most fully on `main`, but it is still **non-normative research**, not an allocated stable or experimental successor epoch by itself.

FCP-0003 / Candidate-1 disposition remains a maintainer decision under issue #13.

A future `UCOF-EXP-0003` should be allocated only after the byte-significant policy package is internally consistent and explicitly accepted for experimentation.

## What `main` now demonstrates

### Bounded hostile-input core behavior

The repository demonstrates:

- checked offset/range arithmetic;
- caller-controlled read, byte, allocation, object, page, depth, and diagnostic budgets;
- strict exact-end validation;
- separate diagnostic and salvage/recovery assurance modes;
- bounded slice, seekable, and random-access source handling;
- Rust 1.85 MSRV checks;
- 32-bit and big-endian portability checks;
- continuous fuzz/property/adversarial evidence.

### Immutable-page directory and identity model

Current research evidence includes:

- immutable content-addressed pages;
- authenticated object records and locators;
- separate object, page, snapshot, and commit/file-instance identity scopes;
- authenticated lookup and absence;
- object/object and object/structural overlap rejection;
- unknown-required capability handling;
- byte-preservation requirements for unknown optional extension data where rewrite APIs claim preservation.

### Canonical occupancy

The current research implementation enforces:

- half-full non-root leaf occupancy;
- half-full non-root internal occupancy;
- root exceptions;
- deterministic final-two-page redistribution;
- strict occupancy validation;
- shared canonical grouping for full construction and canonical mixed reconstruction.

This is implemented for the current research geometry. Whether the same policy is normative for EXP-0003 remains open under issue #16 because EXP-0003 may adopt different identifier/locator widths and therefore different capacities.

### Persistent copy-on-write mutation

The consolidated Rust implementation now includes:

- replacement-only persistent batches;
- persistent insertion through one authenticated path;
- deterministic leaf/internal split propagation;
- root-height growth;
- persistent deletion;
- deterministic left-first borrow, right-borrow fallback, merge fallback, recursive internal repair, and root collapse;
- shared multi-`Put` planning;
- canonical mixed insertion/replacement/deletion batches;
- exact page body reuse where safe;
- page-written/page-reused accounting;
- bounded append-tail streaming variants;
- unified streaming dispatch.

Issue #9 is therefore closed as implementation-complete after the PR #75 audit.

### Source-backed mutation and output

The repository also contains research implementations for:

- verified source copying;
- strong-version source copying;
- source-backed replacement planning;
- source-backed insertion planning;
- source-backed deletion planning;
- source-backed multi-`Put` planning;
- source-backed canonical mixed planning;
- bounded source-to-sink output;
- selected active/historical output;
- bounded selected-history output;
- per-historical-state semantic selection planning.

These provide strong evidence that the core can operate without requiring complete-file materialization, but they do not by themselves qualify real HTTP/cloud provider semantics.

### History and recovery

Current evidence keeps the assurance modes separate:

**Strict active validation**
- exact-end;
- never invokes recovery;
- validates only the claimed active state under the selected assurance contract.

**Linked history**
- explicitly requested;
- revalidates linked prefixes;
- checks parent/sequence/footer relationships;
- can reject corrupted ancestry even when the newest active state is valid.

**Recovery**
- explicitly requested;
- independently bounded;
- treats footer magic as a hint rather than authority;
- returns validated candidate prefixes;
- never silently selects a replacement active state.

### Rewrite and semantic-compaction inputs

The repository supports verified rewrite and selected rewrite, and contains substantial semantic-compaction research.

The core cannot infer arbitrary application dependency semantics from opaque payloads. The intended boundary is:

- core defines structural validity and structural reachability;
- profiles/applications define semantic dependency edges;
- unknown semantic dependencies fail closed or trigger explicit conservative retention policy;
- compaction/rewrite produces new identity;
- byte-scoped signatures are not falsely reported as preserved after rewrite.

Normative application-profile adoption remains open.

### Transport policy research

The repository contains reusable evidence for:

- strong source-version tokens;
- conditional range response validation;
- operation-wide retry budgets;
- bounded backoff planning;
- cooperative wait execution;
- one explicitly authorized authentication refresh;
- cancellation/deadline/version-change classifications;
- freshness-policy separation from mere stable-source integrity.

This is not yet a maintained production HTTP/cloud adapter. Issue #10 remains open for real adapter qualification and native asynchronous cancellation.

### Writer staging and publication research

Current research includes:

- bounded external sorting and descriptor-limited merging;
- deterministic final output across run-size/fan-in variations;
- private staging contracts;
- no-overwrite publication modeling;
- Unix research staging/publication;
- destination-directory identity pinning;
- fault injection and restart classification evidence;
- explicit not-published / publication-indeterminate / published-durable distinctions.

This does not establish universal production durability. Issue #11 remains open for encrypted spill, descriptor-relative hardening, authenticated journaling, fault qualification, and platform/filesystem-specific durability evidence.

## What `main` does **not** establish

Green implementation evidence does not mean that UCOF has:

- a stable wire format;
- an accepted FCP-0003;
- an allocated EXP-0003;
- a compatibility promise;
- a production-qualified durable publication model;
- maintained HTTP/cloud adapters;
- native asynchronous transport cancellation;
- independent implementation agreement;
- application-profile adoption;
- signatures/provenance semantics;
- encryption/selective disclosure semantics;
- Archive/Table profile conformance;
- UCOF 1.0 readiness.

Integrity also does not establish authenticity, freshness, authorization, provenance, confidentiality, or rollback resistance by itself.

## Current Phase 3 convergence gates

### P0 — FCP-0003 and successor epoch decision

Primary tracker: #13.

Required outcomes:

- explicit Candidate 1 disposition;
- review of the proposed identifier/locator/page policy;
- occupancy/split/deletion/batch policy decision;
- exact identity and capability semantics;
- explicit migration non-promises;
- decision whether/when to allocate `UCOF-EXP-0003`.

### P0 — EXP-0003 occupancy/vector convergence

Primary tracker: #16.

The current research geometry already implements canonical half-full occupancy. The remaining gate is to fix the policy for the accepted EXP-0003 layout and regenerate authoritative cross-language vectors from the new capacities.

### P0 — authoritative EXP-0003 specification and compact corpus

Primary tracker: #76.

Required outputs:

- one independently implementable experimental byte specification;
- explicit canonical construction/mutation algorithms;
- explicit validation/assurance semantics;
- compact valid interoperability corpus;
- compact invalid/adversarial corpus;
- exact expected identities and semantic facts.

### P1 — independent implementation or external clean-room review

Primary tracker: #12.

This remains a hard Phase 3 exit gate. Rust plus an in-repository Python implementation can share the same misunderstanding. Material disagreements must be recorded and classified rather than silently changed to match the reference implementation.

### P1 — maintained remote adapters

Primary tracker: #10.

At least one HTTP adapter and one immutable-version cloud-object adapter should be qualified with strong version semantics, malformed-response handling, operation-wide budgets, retries, deadlines, and native asynchronous cancellation.

### P1 — production-candidate publication subsystem

Primary tracker: #11.

Required work includes encrypted spill when policy requires it, descriptor-relative filesystem hardening, authenticated restart/journal semantics, bounded cleanup, and platform/filesystem-specific durability qualification.

### P1 — semantic compaction/profile boundary

The existing semantic/profile research must converge into an explicit core-versus-profile dependency contract and authoritative profile-defined vectors without making profile semantics mandatory core behavior.

## Phase 3 exit rule

Phase 3 is complete only when all of the following are true:

1. one successor experimental epoch is explicitly selected for interoperability work;
2. its byte-significant policies are documented normatively enough for independent implementation;
3. authoritative valid and invalid corpora are published;
4. the consolidated Rust implementation matches those rules and vectors;
5. an independent implementation or external clean-room review satisfies issue #12;
6. selected object lookup remains bounded and avoids unrelated payload reads;
7. interrupted append never silently promotes damaged/recovered state;
8. linked history and report-only recovery remain distinct assurance claims;
9. repair/rewrite/compaction preservation and identity rules are explicit;
10. real remote-source behavior is qualified through maintained adapters;
11. large-writer/publication behavior has a documented production-candidate contract;
12. continuous fuzzing, property, portability, adversarial, and vector evidence remains green;
13. material rejected alternatives and compatibility non-promises are recorded;
14. FCP/Candidate-1 maintainer disposition is committed.

The intended milestone name is **EXP-0003 Interoperability Candidate**.

EXP-0003 remains disposable and must not claim UCOF 1.0 compatibility.

## Next phases

Only after the successor interoperability candidate is coherent should feature phases resume:

- Phase 4 — transform pipeline and compression;
- Phase 5 — schemas and lossless diagnostic text projection;
- Phase 6 — signatures, provenance, and trust/freshness scopes;
- Phase 7 — encryption and selective disclosure;
- Phase 8 — Archive and Table profiles as the first universality proof;
- Phase 9 — broader interoperability, conformance, benchmarks, and hardening;
- Phase 10 — specification freeze and UCOF 1.0.

## Historical evidence

Earlier versions of this document recorded the detailed stacked-PR implementation progression before PR #75. That history remains available in Git and in the closed Phase 1–3 pull requests. It should be treated as research provenance rather than current branch topology.
