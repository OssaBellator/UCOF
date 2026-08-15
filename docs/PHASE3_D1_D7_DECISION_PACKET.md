# Phase 3 D1–D7 maintainer decision packet

**Status:** all decisions intentionally **UNSELECTED**  
**Scope:** governance/convergence support for FCP-0003 / EXP-0003  
**Normative effect:** none until maintainers explicitly select a decision and apply the coordinated amendment

This packet is designed to turn the remaining Phase 3 normative work into seven explicit maintainer decisions instead of allowing implementation evidence, test vectors, or historical candidate text to select policy by accident.

The implementation stack may constrain what is practical, but a green implementation is not itself a normative vote.

## How to use this packet

For each decision D1–D7, maintainers should record:

1. the selected option in precise prose;
2. the rejected alternatives and why;
3. the implementation/evidence relied on;
4. the exact normative sections/algorithms/tables affected;
5. the exact corpus/vector consequences;
6. whether the choice changes externally observable bytes, validation, mutation, or interoperability behavior;
7. any dependency on another D decision;
8. the date and maintainer review reference.

Do **not** update one normative section at a time while the other D decisions remain ambiguous. After D1–D7 are selected, apply one coordinated amendment and regenerate one fresh EXP-0003 candidate corpus from that complete selection.

---

## D1 — Candidate 1 disposition

**Current selection:** UNSELECTED

### Decision question

What status does the existing Candidate 1 material have after Phase 3 convergence?

The choice must distinguish at least:

- historical research evidence that remains useful but is not the selected byte definition;
- material retained unchanged as part of the selected definition;
- material superseded by a new coordinated EXP-0003 candidate;
- any compatibility or migration statement, if one is intentionally made.

### Evidence already available

- Candidate 1 and subsequent Phase 3 experiments expose ambiguities that the D1–D7 process is intended to remove.
- The current implementation stack intentionally avoids treating historical candidate agreement as a stable-format promise.
- 0176–0179 consolidation is implementation evidence, not Candidate 1 ratification.

### Must be pinned if selected

- exact repository status/label for Candidate 1;
- whether its corpus remains a historical fixture or a conformance target;
- whether any byte equality with the new candidate is accidental, preserved by design, or explicitly not promised;
- whether readers/writers must accept Candidate 1 after a later allocation decision.

### Dependencies

D1 should be finalized after the byte-affecting implications of D2–D7 are understood. Otherwise “retain Candidate 1” is not a well-defined promise.

---

## D2 — ObjectId and geometry

**Current selection:** UNSELECTED

### Decision question

What exact ObjectId representation and structural geometry define the candidate format?

This decision must pin every geometry property that can affect canonical bytes, object addressing, bounds, traversal, mutation, or validation.

### Evidence already available

- bounded parsing/writing and authenticated tree experiments provide executable evidence for concrete widths and structural limits;
- encrypted private-stage work shows which geometry is public by design versus merely implementation-private;
- remote-source verification relies on stable object identity/geometry assumptions when locating immutable payload views.

### Must be pinned if selected

- ObjectId width/encoding and canonical comparison/order;
- object/record geometry and all size/count bounds that affect validity;
- tree/page geometry used by canonical encoding;
- overflow behavior and impossible/reserved encodings;
- whether geometry is globally fixed, profile-selected, or encoded in the object;
- exact invalid-vector cases for off-by-one and overflow boundaries.

### Dependencies

- D3 occupancy/split rules consume D2 geometry.
- D6 hash/domain/kind inputs may encode D2 widths/kinds.
- D7 determinism must state which D2 geometry inputs are part of canonical construction.

---

## D3 — Occupancy and split rules

**Current selection:** UNSELECTED

### Decision question

What exact occupancy constraints and deterministic split/redistribution rules define canonical authenticated structures?

### Evidence already available

- persistent authenticated-tree experiments exercise insertion/mutation and structural verification;
- bounded writer/tree-stage work demonstrates deterministic construction under concrete staging limits;
- deletion-policy experiments expose cases where occupancy policy and deletion repair interact.

### Must be pinned if selected

- minimum/maximum occupancy for every non-root node/page kind;
- root exceptions;
- exact split point for every count/parity case;
- sibling/redistribution preference ordering if redistribution exists;
- deterministic tie-breaking when more than one legal repair exists;
- underflow/overflow handling and invalid states;
- insertion/deletion vector cases that hit every occupancy boundary.

### Dependencies

- requires D2 geometry;
- interacts directly with D4 deletion policy;
- feeds D7 determinism because multiple legal tree shapes are unacceptable unless one is selected canonically.

---

## D4 — Deletion policy

**Current selection:** UNSELECTED

### Decision question

What deletion semantics are part of the selected candidate, including structural repair and any policy/reward choice used to choose a parent/sibling/action?

### Evidence already available

- deletion-policy trace examples, trace matrices, reward decomposition, candidate vectors, source-policy I/O and parent-reward catalogs exist in the repository;
- the local acceptance surface already runs these policy/vector checks;
- mutation experiments provide executable tree-level behavior.

### Must be pinned if selected

- logical meaning of deletion and whether tombstone-like states exist;
- exact underflow repair order;
- merge vs redistribute decision rule;
- sibling/parent selection and deterministic tie-breaks;
- root collapse behavior;
- all reward/score components if a scored policy is retained;
- behavior when candidate actions have equal scores;
- complete delete-policy corpus derived from the selected rule rather than historical examples.

### Dependencies

- consumes D2 geometry and D3 occupancy;
- may depend on D5 catalog semantics if catalog entries participate in deletion;
- must be included in D7 canonical determinism.

---

## D5 — Catalog semantics

**Current selection:** UNSELECTED

### Decision question

What exactly does the catalog represent, how is it ordered/identified, and what validation/mutation semantics does it have?

### Evidence already available

- source and mutation experiments exercise authenticated lookup, linked history and object selection;
- remote immutable-source work demonstrates why resource identity, immutable payload-view identity and logical catalog identity must not be conflated;
- 0178 explicitly keeps external source-set identity application/provider-owned rather than deriving it accidentally from payload version tokens.

### Must be pinned if selected

- catalog entry identity and canonical ordering;
- duplicate/conflict semantics;
- relationship between catalog identity, ObjectId, source resource identity and immutable payload version;
- whether catalog contents are authoritative, advisory, derived, or independently authenticated;
- insertion/removal/update semantics;
- validation behavior for missing, extra, duplicated or reordered catalog material;
- corpus cases for every catalog ambiguity.

### Dependencies

- depends on D2 ObjectId representation;
- may affect D4 deletion semantics;
- feeds D6 domains/kinds and D7 deterministic ordering.

---

## D6 — Hash, domain-separation and kind choices

**Current selection:** UNSELECTED

### Decision question

What exact cryptographic hash construction, domain-separation inputs and kind/tag values are part of the selected byte definition?

This decision is normative even when multiple choices are cryptographically reasonable: interoperability requires one exact construction.

### Evidence already available

- authenticated structure, history, restart and source experiments demonstrate extensive use of explicit identity/digest domains;
- implementation work has repeatedly exposed the value of keeping identity domains separate rather than reusing a convenient token for a different meaning;
- private HMAC/AEAD mechanisms are implementation-security evidence and must not silently determine public wire hash domains.

### Must be pinned if selected

- public hash algorithm and output width;
- exact preimage framing;
- every domain-separation constant/tag and its byte encoding;
- every object/node/catalog/history kind tag and reserved space;
- whether length/type/version fields are included and in what order/endianness;
- invalid behavior for unknown/reserved kinds;
- known-answer vectors for each domain and cross-domain non-equivalence cases.

### Dependencies

- consumes D2 object/geometry fields and D5 catalog kinds;
- D7 must define the exact canonical inputs to these hashes.

---

## D7 — Determinism boundary

**Current selection:** UNSELECTED

### Decision question

Which inputs are permitted to influence canonical public bytes, and which implementation/environmental facts are explicitly excluded?

This is the cross-cutting decision that prevents two conforming writers from producing different public objects while both believing they followed the specification.

### Evidence already available

- bounded writer experiments reproduce canonical bytes across different spill/run/fan-in choices;
- encrypted private stages demonstrate that private ciphertext/nonces can vary while public canonical output remains identical;
- remote-source work separates transport/provider metadata from canonical payload interpretation;
- restart/publication experiments demonstrate that crash history, private filenames and fresh nonce generations must not alter canonical public output.

### Must be pinned if selected

- complete list of semantic inputs to canonical construction;
- canonical ordering for every unordered/set-like input;
- whether source enumeration order is semantic or normalized;
- treatment of duplicate equal inputs;
- explicit exclusion of private spill layout, temporary filenames, process/thread scheduling, random nonces, restart count, provider request ordering, filesystem metadata and wall-clock time unless intentionally specified;
- deterministic tie-breaking for D3/D4/D5 choices;
- cross-implementation corpus cases proving the same semantic input produces the same bytes under different private execution plans.

### Dependencies

D7 consumes the final selections of D2–D6 and should be reviewed last, even if its principles guide all earlier decisions.

---

## Coordinated amendment checklist

After D1–D7 are explicitly selected, the amendment should be prepared as one reviewable change set:

- [ ] update every normative definition/algorithm/table affected by D1–D7;
- [ ] remove or clearly label superseded candidate language;
- [ ] ensure terminology uses one identity domain for one meaning;
- [ ] update normative validation rules at every exact boundary;
- [ ] regenerate the EXP-0003 candidate corpus from the selected rules;
- [ ] regenerate invalid vectors for every newly pinned boundary/reserved encoding;
- [ ] run implementation/model/corpus checks without treating same-repo implementations as independent confirmation;
- [ ] produce a compact maintainer decision record linking each normative diff back to D1–D7;
- [ ] request clean-room interpretation/reproduction from outside the implementation lineage;
- [ ] only after that evidence, consider the separate FCP status/allocation decision.

## Clean-room handoff package

The independent implementation/review request should contain only the selected normative material and public corpus needed to reproduce the format. It should not require reading the Rust/Python implementation to discover missing rules.

A useful acceptance signal is not merely “their bytes match ours”; it is that an independent reader can explain D1–D7 from the normative text alone, implement them without private clarification, and either reproduce the corpus or identify a specification ambiguity that the repository can correct.

## Non-claims

This packet does not:

- select any D decision;
- recommend allocating EXP-0003;
- move FCP-0003 out of Draft;
- promote Experiment 0179;
- make Candidate 1 stable or obsolete by itself;
- create a compatibility promise.
