# Experiment 0135 — EXP-0003 ObjectId scope before width

**Status:** non-normative research evidence  
**Date:** 2026-08-13  
**Related:** Experiment 0108, FCP-0003, issues #13, #16, #76

## Question

The first self-contained EXP-0003 Draft proposes a 16-byte opaque `ObjectId`, while the current Rust research microformat uses 64-bit identifiers.

Experiment 0108 showed that identifier width materially changes object-header density, leaf-entry width, internal-reference width, page capacity, and total structural bytes. It also showed a large difference in birthday-collision probability if identifiers are generated independently at random.

Those facts do not answer the normative question until the **identifier scope contract** is explicit.

This experiment separates three materially different contracts:

1. coordinated local allocation inside one output/container context;
2. independently generated random identifiers intended to coexist without coordination;
3. independent local namespaces that later need to be combined without remapping.

The mathematics and the required width differ among those regimes.

## Existing Draft semantics

The first EXP-0003 Draft already says that `ObjectId` is an opaque lookup key rather than a content digest, signature identity, or globally guaranteed unique name.

That language is closer to a local structural key than to a UUID-style global identity.

For comparison, [RFC 9562 §6.8](https://www.rfc-editor.org/rfc/rfc9562.html#section-6.8) explicitly distinguishes local uniqueness from broader UUID uniqueness use, and UUIDs are 128 bits so they can be generated without a central registration process. That is a stronger distributed-generation goal than the current EXP-0003 Draft claims for `ObjectId`.

## Physical cardinality bound for coordinated local IDs

EXP-0003 physical offsets are `u64` absolute byte offsets.

For a conservative upper bound, suppose an entire `u64` byte address space contained nothing except minimum-size object records. This deliberately ignores bootstrap, directory pages, snapshots, footers, payloads, alignment, and history, so a real UCOF file can only contain fewer objects.

The executable evaluates compact 64-bit, compact 128-bit, and first-Draft header sizes:

| Minimum object header | Maximum minimum-size records in `u64::MAX` bytes | Fraction of nonzero 64-bit ID space | ID-space / physical-record bound |
|---:|---:|---:|---:|
| 48 bytes | 384,307,168,202,282,325 | 2.0833% | 48× |
| 56 bytes | 329,406,144,173,384,850 | 1.7857% | 56× |
| 64 bytes | 288,230,376,151,711,743 | 1.5625% | 64× |

Thus even the densest candidate object record cannot physically consume more than about one forty-eighth of the nonzero 64-bit identifier space before the `u64` byte-address space is exhausted.

This means **64-bit identifier cardinality is not the limiting resource for a coordinated file-local namespace**. Directory pages, payloads, snapshots, and history make the practical margin larger.

This argument says nothing about probabilistic collision under independent generation; it applies only when the output allocator/checker coordinates uniqueness.

## Independent random generation

For uniformly independent random identifiers, the executable uses the standard Poisson birthday approximation:

```text
P(any collision among n IDs)
  ~= 1 - exp(-n(n-1) / (2 * 2^b))
```

where `b` is the identifier width.

| Bits | Objects | Approx. collision probability |
|---:|---:|---:|
| 64 | 1,000,000 | 2.7105e-8 |
| 64 | 100,000,000 | 2.71014e-4 (~0.0271%) |
| 64 | 1,000,000,000 | 2.6741e-2 (~2.67%) |
| 128 | 1,000,000 | 1.46937e-27 |
| 128 | 100,000,000 | 1.46937e-23 |
| 128 | 1,000,000,000 | 1.46937e-21 |

If EXP-0003 requires uncoordinated producers to mint structural ObjectIds independently and later place them into one namespace without remapping, 64-bit random generation is not a comfortable billion-object contract. A 128-bit random space makes that collision risk negligible for the modeled scales.

## Cross-namespace combination

The no-remap merge problem is different from within-file birthday probability.

For two independently random sets of sizes `a` and `b`, the executable uses:

```text
P(any cross-set collision)
  ~= 1 - exp(-ab / 2^bits)
```

For equal-size sets:

| Bits | Left | Right | Approx. cross-collision probability |
|---:|---:|---:|---:|
| 64 | 1,000,000 | 1,000,000 | 5.42101e-8 |
| 64 | 100,000,000 | 100,000,000 | 5.41954e-4 (~0.0542%) |
| 64 | 1,000,000,000 | 1,000,000,000 | 5.27669e-2 (~5.28%) |
| 128 | 1,000,000 | 1,000,000 | 2.93874e-27 |
| 128 | 100,000,000 | 100,000,000 | 2.93874e-23 |
| 128 | 1,000,000,000 | 1,000,000,000 | 2.93874e-21 |

But identifier width alone does **not** solve combination for local namespaces. If two independent files both allocate dense local IDs beginning at one, combining equal ranges without remapping produces a deterministic conflict for every overlapping ID whether the field is 64 or 128 bits wide.

For example:

```text
file A IDs = 1..=100,000,000
file B IDs = 1..=100,000,000

no-remap conflicts = 100,000,000
```

Changing the field from 64 to 128 bits does nothing unless the generation contract also changes.

## Opaque payloads make generic remapping a semantic operation

UCOF core treats application payloads as opaque bytes.

A generic core combiner therefore cannot assume it can rewrite every semantic reference to an ObjectId embedded inside payloads, schemas, indexes, or profile data. Safe collision remapping requires profile/application knowledge or an explicit external-reference/identity mechanism.

That leads to an important architecture boundary:

> Cross-file identity preservation and semantic merge should not be silently inferred from the width of the core structural ObjectId.

If a profile needs a globally portable semantic identifier, that identity can be carried in profile-defined metadata/payloads independently of the compact structural lookup key.

## Recommended scope contract for EXP-0003 review

The evidence supports making the following scope explicit before choosing 64 versus 128 bits:

1. `ObjectId` is a **container-context structural lookup key**, not a globally unique semantic identity.
2. A valid active snapshot requires its ObjectIds to be unique within that snapshot's primary directory.
3. Preserving the same ObjectId across persistent updates/rewrite preserves the structural lookup slot but does not create a global cross-file identity claim.
4. EXP-0003 core makes **no generic no-remap merge guarantee** for independently created containers.
5. Combining containers with colliding ObjectIds is a profile/application operation: reject, namespace externally, or remap using semantics capable of updating all affected references.
6. Independently generated application/global IDs, when needed, belong in profile/application semantics rather than being inferred from core ObjectId.
7. The core must not advertise birthday-collision probabilities as its uniqueness guarantee if the accepted writer contract instead requires duplicate detection/coordinated assignment in the output namespace.

This scope matches UCOF's broader principle: universal container, not universal representation.

## Consequence for width review

Once the above local structural scope is accepted, the width decision becomes much cleaner:

- **64 bits** has ample coordinated local cardinality even under the conservative `u64` physical-address bound and gives materially better density;
- **128 bits** buys practical safety for uncoordinated random generation/no-remap coexistence, but that is a stronger contract than the proposed local structural scope;
- retaining 128 bits while explicitly disclaiming global/no-remap identity is a valid conservatism choice, but the format should acknowledge that it is paying a persistent density cost for flexibility rather than for required local cardinality.

Experiment 0108's 100-million-object first-order structural model gives:

```text
compact 128-bit: ~12.054 GB structural bytes
compact 64-bit:  ~10.450 GB structural bytes
```

The difference includes the corresponding object-header, leaf-entry, page-header, and internal-entry geometry changes; it is not merely eight bytes multiplied by object count.

This experiment does **not** select 64 bits yet. The next geometry experiment should combine the scope result with internal-child-reference minimization, because removing redundant child minima changes how much of the 64-vs-128 density gap remains in internal pages.

## Why not make core ObjectId a UUID

UCOF can carry UUIDs as application/profile identities where a UUID contract is appropriate. Requiring every primary-directory lookup key to be a UUID would conflate two layers:

- structural identity optimized for one authenticated container state;
- semantic/global identity intended to survive distribution and cross-container composition.

RFC 9562's 128-bit UUID design is excellent evidence for the latter use case, not proof that the former must always consume 128 bits.

## CI assertions

`tools/experiment_exp0003_object_id_scope.py` runs in the normal experiment block and requires:

- the nonzero 64-bit ID space to exceed the conservative maximum number of minimum-size object records by at least 48×;
- 64-bit random birthday risk to become material at 100M–1B objects;
- 128-bit random birthday risk to remain negligible at the modeled scales;
- 64-bit random cross-namespace collision risk to exceed 5% for two 1B-object sets;
- dense local namespaces with the same origin to conflict deterministically regardless of width.

## Boundary

This is namespace/geometry evidence only.

It does not:

- change the first EXP-0003 Draft's 16-byte ObjectId;
- select 64-bit ObjectIds;
- define an allocator algorithm;
- promise global identity;
- define cross-file merge semantics;
- accept FCP-0003;
- allocate EXP-0003;
- regenerate authoritative vectors.

The next review step is to test compact 64/128-bit geometry together with a smaller authenticated internal routing reference before a width recommendation is folded into the Draft.

## Reproduction

```console
python3 tools/experiment_exp0003_object_id_scope.py
python3 tools/experiment_exp0003_id_width.py
```
