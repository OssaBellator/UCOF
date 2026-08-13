# Experiment 0138 — History-independent dynamic partitioning applicability to EXP-0003

**Status:** reproducible architectural evidence  
**Date:** 2026-08-13  
**Related:** Experiment 0111; identifier/geometry packet #113; hash/magic/kind packet #115; issues #13, #16, #76

## Question

Experiment 0111 concluded that the simple history-independent partition families tested there did not simultaneously provide:

- current-set canonical partition identity;
- hard fixed-page maximum;
- half-full minimum occupancy;
- local persistent updates;
- cross-language deterministic bytes;
- robustness when writers can choose `ObjectId`s.

It also noted newer history-independent dynamic-partitioning research and explicitly left open whether that work changes the case for EXP-0003's proposed **scoped determinism**:

```text
fresh canonical rewrite: current-set canonical
persistent append mutation: deterministic from prior bytes + canonical batch,
                            but root/page identity may remain history-sensitive
```

This experiment closes that research gap narrowly.

It asks:

1. Does the newer construction solve the fixed maximum/minimum occupancy problem relevant to UCOF?
2. Does it, by itself, make EXP-0003 root/snapshot bytes history-independent under the current offset-bearing immutable-page design?
3. Are its update-locality guarantees directly portable to a public deterministic `ObjectId` namespace?

## Primary research reviewed

The primary result is:

Michael A. Bender, Martín Farach-Colton, Michael T. Goodrich, and Hanna Komlós, **History-Independent Dynamic Partitioning: Operation-Order Privacy in Ordered Data Structures**, PACMMOD/PODS 2024, DOI `10.1145/3651609`; a minor revision appeared as a SIGMOD Record Research Highlight in 2025.

The 2026 extension is:

Michael A. Bender, Martín Farach-Colton, Michael T. Goodrich, and Hanna Komlós, **History-Independent Dynamic Partitioning with Applications to B-Trees, Skip Lists and Fusion Trees**, ACM TODS 2026, DOI `10.1145/3810240`.

The 2025 SIGMOD Record version is especially useful because it exposes the construction and B-tree mapping in an accessible primary paper.

## What the result actually establishes

The paper defines strong history independence so that, **after initial random bits have been fixed**, each logical state has a canonical representation.

For dynamic partitioning it initializes a random hash function `h` before construction. For a fixed `h` and size parameter `B`, the partition is uniquely determined by the current ordered set.

The main `(B, 2)` result gives deterministic group cardinality:

```text
B/2 <= group_size <= B
```

There is no probability that a group violates the bound. Randomness is used for locality/performance analysis, not for whether the resulting groups fit the stated cardinality envelope.

The history-independent B-tree then applies the partition recursively at every level, using independent random hash functions `h0`, `h1`, ... for successive levels. Every B-tree node contains between `B/2` and `B` elements.

The expected update-cost bounds are proved against an **oblivious adversary**; the paper reports constant expected element movement for partitioning and a B-tree update overhead of `O(log_B(N)/B)` I/Os in expectation beyond search, with high-probability bounds as well.

The paper also states that making the B-tree fully history independent requires a history-independent memory allocator in addition to history-independent balance/partition structure.

## Result 1 — the occupancy obstacle is real and is solved in theory

If the tight 64-bit Review candidate from #113 is selected, EXP-0003 would have:

```text
page capacity C = 291
minimum M       = 146
```

Setting the research partition parameter to:

```text
B = 291
```

gives the integer cardinality envelope:

```text
B/2 <= |G| <= B
145.5 <= |G| <= 291
```

and therefore, for integer group sizes:

```text
146 <= |G| <= 291
```

which exactly matches the proposed half-full non-root occupancy interval.

This changes the interpretation of Experiment 0111 materially:

> History-independent dynamic partitioning with the exact UCOF-style half-full/hard-maximum envelope is not merely hypothetical. A primary research construction now exists.

Therefore EXP-0003 must **not** justify scoped determinism by claiming that efficient history-independent half-full B-tree partitioning is mathematically unavailable.

## Result 2 — partition canonicality does not imply UCOF root-byte canonicality

EXP-0003 page identity authenticates exact page bytes.

Those bytes currently include absolute physical placement:

### Leaf locator

```text
ObjectId
object_record_offset
object_record_length
object_record_digest
```

### Internal child reference

```text
child_minimum_ObjectId
child_maximum_ObjectId
child_page_offset
child_page_digest
```

### Snapshot

```text
root_page_offset
root_page_digest
...
```

The exact widths may change under #113, but all current Review candidates retain the absolute offset fields.

That creates a placement-sensitive identity chain:

```text
same object record bytes
+ different object physical offset
-> different leaf locator bytes
-> different leaf page bytes/digest
-> different ancestors/root digest

same child page bytes/digest
+ different child physical offset
-> different parent reference bytes
-> different parent page bytes/digest
-> different root digest

same hypothetical root page digest
+ different root physical offset
-> different snapshot bytes/digest
```

A canonical partition algorithm does not remove any of these offset fields.

## Reproducible offset obstruction

`tools/experiment_exp0003_offset_identity_obstruction.py` constructs the same logical object/page relationships twice while changing only physical offsets.

It pins the property for:

- the current first-Draft 128-bit geometry;
- the tight 64-bit Review candidate from #113.

Expected summary:

```text
geometry,object_digest_same,leaf_bytes_same,leaf_digest_same,internal_bytes_same,internal_digest_same,snapshot_digest_same
draft-128,1,0,0,0,0,0
tight-64-review-candidate,1,0,0,0,0,0
```

Interpretation:

- object record identity is placement-independent because object offset is outside object-record bytes;
- primary-directory page identity is placement-dependent because locators/references authenticate offsets;
- snapshot identity is separately placement-dependent because the snapshot authenticates the root offset.

This is a byte-format fact, not a stochastic model.

## Relation to the research allocator requirement

The primary paper explicitly separates history-independent B-tree balance from physical placement and notes that full history independence additionally needs a history-independent memory allocator.

That maps directly onto UCOF's obstruction.

EXP-0003 is not an in-place allocator-backed B-tree. Its Phase 3 design intentionally uses:

- immutable pages;
- append-only commit construction;
- reuse of unchanged historical page/object bytes at their existing physical offsets;
- authenticated absolute offsets for bounded/random-access planning and validation.

Those properties make physical history visible in active authenticated structure by design.

Therefore adopting the research partitioning primitive alone would **not** make equal logical active sets produce equal EXP-0003 root digests under persistent mutation.

## What would have to change for canonical persistent root identity

At least one additional architectural change is required.

### Option A — canonical physical reallocation after every logical change

Re-emit active objects/pages into one current-set canonical physical order and recompute all offset-bearing references.

This can produce reproducible bytes but largely gives up the immutable-page/object reuse that motivates persistent append mutation.

It is essentially canonical rewrite as the normal mutation path.

### Option B — remove physical offsets from authenticated logical page identity

For example, make tree references content-addressed/logical and move physical location into a separate lookup structure.

This is a substantial wire redesign:

- targeted range planning changes;
- bootstrapping physical lookup changes;
- duplicate-content/reference policy changes;
- repair/recovery changes;
- another index may itself need history-independent identity.

This is not a small FCP-0003 amendment.

### Option C — add a history-independent physical allocator/placement scheme

A deterministic allocator could make logical nodes land at canonical positions.

In an append-only immutable lineage, however, preserving old bytes at old offsets while assigning new bytes canonical global positions is inherently constrained. A full allocator design would need to be specified and independently reproduced.

That is outside the current Phase 3 wire experiment.

### Option D — define a separate logical-state identity

Keep physical root/snapshot identity history-sensitive, but define an application/profile logical-state digest over canonical logical facts rather than physical offsets.

This matches the current scoped-determinism direction: structural byte identity is not semantic state identity.

## Result 3 — random-hash initialization is another interoperability dimension

The history-independent partition is canonical **conditional on fixed initial random hash functions**.

The B-tree construction uses an independent hash function at each level.

For UCOF to turn that into exact cross-language writer bytes, the hash functions/seed derivation would themselves need normative encoding or derivation.

Possible choices each add a trade-off:

### One public epoch-fixed priority function

Example conceptually:

```text
priority(level, ObjectId) = SHA-256(domain || level || ObjectId)
```

This gives every implementation the same partition without file-specific seed bytes.

But the paper's expected locality results assume a random hash and are stated against an oblivious adversary. A writer/user that can inspect the public priority function and choose `ObjectId`s adaptively is a stronger adversary.

Experiment 0111 already demonstrated substantial chosen-ID steering for simpler public hash-priority partition families. That is not a proof against the protected-Cartesian construction, but it is enough to prevent importing its **expected update-cost guarantee** unchanged without a new adversarial analysis/experiment.

Correct group bounds remain deterministic; the concern is locality/write amplification, not page validity.

### File/lineage random public seed

Store a random partition seed in bootstrap/catalog metadata and derive per-level hash functions from it.

Then the representation is canonical only conditional on that seed. Equal logical sets in independently created lineages need not have equal root identity unless the seed is also treated as part of the logical state.

Once the seed is published, later writers can also observe it before choosing future IDs, so the oblivious-adversary locality model still requires care.

### Secret writer seed

Keeping the seed secret may preserve unpredictability against key selection, but independent readers/writers cannot reproduce or verify the canonical partition from file bytes alone.

That conflicts with EXP-0003's interoperability purpose.

## Result 4 — full-file history independence is not the EXP-0003 goal anyway

Even if active tree partition and physical allocation were canonicalized, EXP-0003 intentionally retains linked historical commits and older immutable bytes unless a rewrite/compaction removes them.

A file containing explicit retained history cannot simultaneously hide the fact that those historical states existed.

The useful decision is therefore narrower:

> Should the **active structural root representation** be canonical across operation histories?

For the current offset-bearing persistent format, achieving that requires broader physical-reference/placement changes than the partitioning primitive itself.

## Decision impact

Experiment 0138 changes the rationale but supports retaining the current first-epoch scoped-determinism boundary.

### What should no longer be claimed

Do not claim:

- history-independent half-full B-tree partitioning is unavailable;
- canonical dynamic partitioning necessarily requires broad rank-shift rewrites;
- the only choices are packed-by-rank rewrite or history-sensitive split/merge.

The Bender–Farach-Colton–Goodrich–Komlós construction demonstrates otherwise.

### What EXP-0003 can responsibly claim

For this epoch:

1. canonical bulk/rewrite identity remains current-set deterministic under the accepted bulk grouping;
2. persistent append mutation remains deterministic from the exact prior valid state plus canonicalized batch;
3. persistent root/page/snapshot identity may remain history-sensitive;
4. this is a deliberate consequence of authenticated physical offsets and page/object reuse, not a claim that history-independent B-trees are impossible;
5. semantic/profile state identity must not be inferred from persistent structural root digest;
6. a future epoch may revisit history-independent active roots together with offset-free references, canonical placement, or a separately specified logical-state identity.

## Recommended Review disposition

For the first interoperability epoch, retain Section 19's scoped determinism with revised rationale:

> **Persistent EXP-0003 structural identity is history-sensitive because authenticated directory/snapshot bytes contain physical offsets and persistent mutation deliberately reuses existing physical records/pages. Canonical dynamic partitioning alone cannot remove that placement dependence. Fresh canonical rewrite remains the defined current-set canonical structural form.**

Also add a non-normative note that history-independent `(B,2)` partitioning exists and exactly matches a half-full/hard-maximum occupancy envelope, but adopting it would require a broader placement/reference and public-randomness design than EXP-0003 currently specifies.

## Future-epoch research gate

Reopen history-independent persistent root identity only with a proposal that answers all of these together:

1. How are object physical locations represented without making leaf identity history-sensitive?
2. How are child page locations represented without making ancestor identity history-sensitive?
3. How is root location bound without making snapshot identity history-sensitive?
4. What fixed/random per-level priority functions are used?
5. Are those functions public and reproducible across languages?
6. What locality/write-amplification guarantee survives writer-chosen/adaptive ObjectIds?
7. What is the exact `[M,C]` bound for accepted page geometry?
8. How does recovery/repair locate content efficiently?
9. What happens to append-only reuse and historical snapshots?
10. Is the resulting identity structural or a separate logical-state digest?

Until a proposal answers that whole set, swapping only the split/merge partition rule would create complexity without delivering the claimed root-identity property.

## CI assertions

`tools/experiment_exp0003_offset_identity_obstruction.py` runs in the normal experiment block and requires, for both modeled geometries:

- identical object-record digest under pure relocation;
- different leaf bytes/digest when only object offset changes;
- different internal page bytes/digest when only one child offset changes;
- different snapshot digest when only root offset changes.

## Boundary

This experiment does **not**:

- reject history-independent dynamic partitioning as a future design;
- claim the research construction violates occupancy bounds;
- claim its expected locality bounds are false;
- change current Draft bytes;
- select identifier geometry;
- select deletion borrower policy;
- accept catalog/hash decision packets;
- accept FCP-0003;
- allocate EXP-0003;
- regenerate authoritative vectors.

It narrows the first-epoch decision: scoped determinism remains justified by EXP-0003's authenticated physical-reference architecture even though canonical dynamic partitioning is now known to be possible.

## Reproduction

```console
python3 tools/experiment_exp0003_offset_identity_obstruction.py
```
