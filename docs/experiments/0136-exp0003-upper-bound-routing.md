# Experiment 0136 — EXP-0003 upper-bound-only internal routing

**Status:** non-normative research evidence  
**Date:** 2026-08-13  
**Related:** Experiments 0108, 0134, 0135; FCP-0003; issues #10, #13, #16, #76

## Question

The first self-contained EXP-0003 Draft stores this 72-byte internal child reference:

```text
child minimum ObjectId   16
child maximum ObjectId   16
child page offset         8
child page digest        32
                         --
                         72 bytes
```

The parent page header separately stores the parent minimum and maximum, and strict validation recursively authenticates every reachable child page and cross-checks each child's header bounds.

The Draft's **insertion** routing rule is already expressed primarily in terms of child maxima: an ID in a sparse gap routes to the first child whose maximum is greater than the ID, and an ID beyond every maximum routes to the final child.

That does not mean the duplicated child minimum is free to remove. Explicit child minima also let a targeted lookup prove an **inter-child gap absence at the parent** without authenticating another child page.

This experiment therefore asks four separate questions:

1. Is insertion routing equivalent with upper-bound-only entries?
2. Is targeted lookup result-equivalent, and what extra page information does a gap absence require?
3. Can strict recursive validation reconstruct omitted minima and preserve the same final non-overlap checks?
4. What density/height benefit does removing the duplicated minimum buy under compact 128-bit and 64-bit geometry?

## Candidate internal reference

For identifier width `I`, the upper-bound-only entry is:

```text
child maximum ObjectId   I bytes
child page offset        8 bytes
child page digest       32 bytes
```

Thus:

| Identifier width | Full-range compact entry | Upper-bound-only entry |
|---:|---:|---:|
| 128 bits | 72 bytes | 56 bytes |
| 64 bits | 56 bytes | 48 bytes |

The candidate still authenticates the exact child page bytes through the child digest. It removes only the duplicated child minimum from the parent entry.

## Insertion routing equivalence

Let ordered child maxima be:

```text
U0 < U1 < ... < Un
```

For insertion of key `q`, choose the first child with:

```text
q <= Ui
```

and choose the final child when `q > Un`.

For a valid sparse ordered tree this is exactly the current Draft rule:

- inside an existing child range: choose that child;
- in a sparse gap: choose the next child;
- before the first actual minimum: choose the first child;
- beyond every maximum: choose the final child.

The executable generated **992** valid sparse parents with 2–32 children and exhaustively checked **170,947** query positions.

Result:

```text
insertion routing mismatches: 0
```

The proof depends only on total ordering and therefore applies equally to lexicographically ordered fixed-width opaque `ObjectId` bytes.

## Targeted lookup is result-equivalent, not path-equivalent

The current experimental authenticated lookup implementation uses explicit child ranges differently from insertion.

For every authenticated internal page it:

1. validates all stored `[minimum, maximum]` sibling ranges for ordering/non-overlap;
2. selects a child only when the query is actually inside that child's explicit range;
3. returns absence immediately when the query is inside the parent range but in no child range.

See `crates/ucof-experiments/src/exp0002_lookup.rs`.

That means a query in this sparse gap:

```text
left child max < q < right child min
```

can terminate at the current full-range parent.

With upper-bound-only entries, the parent knows the right child's maximum but not its actual minimum. A valid lookup therefore selects the right child by upper bound, authenticates that page, then learns:

```text
q < right_child.min
```

and returns the same absence result.

So the candidate is **lookup-result equivalent on a valid tree but can require one additional authenticated child page for an inter-child gap absence**.

### Deterministic stress-corpus result

Across the 170,947 tested positions:

```text
queries inside one child range:        117,698
inter-child gap absences:               47,680
outside-parent absences:                 5,569
end-to-end lookup result mismatches:          0
in-range selected-child mismatches:           0
full-range parent gap shortcuts:         47,680
upper-bound extra child reads for gaps:  47,680
```

The synthetic corpus deliberately creates sparse gaps of varying sizes. Therefore:

> `47,680 / 170,947 ~= 27.9%` is a **stress-corpus exposure fraction, not a predicted production workload frequency**.

The transport-independent result is narrower and stronger:

```text
one uncached inter-child gap absence
  -> full-range: may stop at authenticated parent
  -> upper-bound: needs one additional child-page authentication
```

At the Experiment 0134 stress cap of 257 bytes per bounded source read, one uncached 16 KiB page corresponds to 64 bounded reads, 16,384 bytes, and 128 strong-version checks under that current source architecture. This is not a claim of 64 network round trips; a maintained adapter allowed to fetch one whole page could map the same information need to one range request.

## Why the local-ID scope matters to gap frequency

Experiment 0135 recommends treating `ObjectId` as a container-context structural key rather than a globally random semantic identity.

That scope can make dense/coordinated allocation practical, which may reduce large inter-child gaps in fresh trees. It does not eliminate gaps:

- deletion creates holes;
- sparse application allocation remains permitted unless the eventual allocator contract forbids it;
- persistent history can move page minima/maxima without making every numeric/key interval dense;
- opaque lexicographic identifiers need not have meaningful arithmetic adjacency.

Therefore the format should not assume gap absences are negligible merely because a coordinated allocator is possible.

## Strict recursive validation

The current full-range entry lets a validator check declared sibling non-overlap from the parent page alone before opening children.

With upper-bound-only entries, strict validation can reconstruct the omitted facts while recursively authenticating child pages:

1. stored child maxima are non-zero and strictly increasing;
2. each stored maximum equals the authenticated child header maximum;
3. first child minimum equals parent minimum;
4. for each later child:

   ```text
   child_min > previous_stored_child_max
   ```

5. final stored child maximum equals parent maximum;
6. expected level, page digest, occupancy, and ordinary page checks still apply.

Section 21 already requires strict validation to recursively authenticate every reachable directory page, so the same **final** non-overlap invariant remains enforceable.

The executable injected **992** overlapping-child cases while leaving stored maxima strictly increasing.

| Check | Full-range parent | Upper-bound parent |
|---|---:|---:|
| Detect overlap from parent metadata alone | 992 / 992 | 0 / 992 |
| Detect overlap after strict child-header authentication | 992 / 992 | 992 / 992 |

Thus the candidate preserves final strict rejection, but loses the current parent-local rejection point.

## Targeted lookup / absence assurance boundary

Section 22 gives targeted lookup a narrower assurance scope than full validation: it authenticates one root-to-leaf path rather than every reachable sibling page.

The reference implementation audit exposes two distinct properties of the current full-range form:

1. **Authenticated routing metadata:** one parent page carries explicit claimed child minima/maxima and can reject overlapping claimed ranges locally.
2. **Gap short-circuit:** a query in no explicit child range can return absence without descending.

A one-path lookup still does not authenticate unopened sibling page bodies, so it cannot independently prove that every sibling header agrees with every stored parent claim. Complete strict validation is what closes that gap.

Upper-bound-only entries preserve deterministic routing and strict validation, but weaken both parent-local properties above.

If Review adopts max-only, Section 22 must say clearly that:

- parent maxima define the authenticated routing partition;
- targeted mode does not verify unopened child-header minima/conformance;
- an inter-child gap absence may require authenticating the selected next child to prove `q < child.min`;
- full global structural validity requires strict validation or an explicitly trusted prior-validation/cache contract.

## Geometry

The experiment keeps 16 KiB pages and compact header candidates from Experiment 0108.

### Compact 128-bit

```text
page header: 64 bytes
leaf entry:  64 bytes
leaf cap:   255

full-range internal:   72 bytes -> fanout 226
upper-bound internal:  56 bytes -> fanout 291
```

### Compact 64-bit

```text
page header: 48 bytes
leaf entry:  56 bytes
leaf cap:   291

full-range internal:   56 bytes -> fanout 291
upper-bound internal:  48 bytes -> fanout 340
```

## Directory-byte effect

Leaf pages dominate the primary directory, so reducing only internal-reference width has a modest steady-state byte effect while tree height stays unchanged.

| Layout | Objects | Full-range directory bytes | Upper-bound directory bytes | Saving | Saving % |
|---|---:|---:|---:|---:|---:|
| compact-128 | 10M | 645,382,144 | 644,743,168 | 638,976 | 0.0990% |
| compact-128 | 100M | 6,453,690,368 | 6,447,284,224 | 6,406,144 | 0.0993% |
| compact-128 | 1B | 64,536,576,000 | 64,472,580,096 | 63,995,904 | 0.0992% |
| compact-64 | 10M | 565,002,240 | 564,723,712 | 278,528 | 0.0493% |
| compact-64 | 100M | 5,649,694,720 | 5,646,876,672 | 2,818,048 | 0.0499% |
| compact-64 | 1B | 56,496,603,136 | 56,468,537,344 | 28,065,792 | 0.0497% |

That is not a large steady-state directory-byte win relative to the new gap-absence information cost.

## Height-threshold effect

The stronger benefit is delaying the next tree level.

Maximum object count representable in **four directory levels** (`leaf + two internal + root`) is:

```text
leaf_capacity * internal_fanout^3
```

| Layout | Full-range fanout | Upper-bound fanout | Full-range four-level cap | Upper-bound four-level cap |
|---|---:|---:|---:|---:|
| compact-128 | 226 | 291 | 2,943,509,880 | 6,283,753,605 |
| compact-64 | 291 | 340 | 7,170,871,761 | 11,437,464,000 |

That is a ~2.13× four-level capacity increase for compact-128 and ~1.60× for compact-64.

At 10 billion objects in the simple packed model:

```text
compact-128 full-range:  5 levels
compact-128 upper-bound: 5 levels
compact-64 full-range:   5 levels
compact-64 upper-bound:  4 levels
```

So max-only can remove an entire page level near a very large scale threshold even though ordinary directory-byte savings remain small.

## Mutation effect

Every changed immutable child changes its digest, so its parent reference must be rewritten whether or not the child's minimum changed. Omitting child minima does **not** remove ordinary parent copy-on-write rewrites.

The candidate benefit is therefore:

- higher internal fanout;
- fewer internal pages;
- higher object-count thresholds before tree height grows;
- smaller internal-entry parsing/state.

The costs are:

- loss of parent-local child-minimum overlap checking;
- one additional child-page authentication for uncached inter-child gap absences;
- more complicated targeted-absence wording.

## Decision consequence

After the reference-implementation audit, the evidence is less favorable to immediate max-only adoption than the raw fanout numbers suggest.

For the **first EXP-0003 interoperability Draft**, a conservative recommendation is now:

> **Retain explicit child minimum + maximum ranges unless Review explicitly prioritizes the multi-billion-object height threshold over parent-local gap absence and supplies a precise targeted-assurance contract for the extra child read.**

Why this default is reasonable:

- steady-state directory-byte savings are only ~0.05–0.10% in the modeled 10M–1B range;
- explicit ranges already provide useful authenticated gap short-circuit behavior in the existing implementation;
- remote/random access is a core UCOF design target;
- full-range fanout is already 226 for compact-128 and 291 for compact-64;
- max-only remains available as a future incompatible epoch/profile choice if real workloads justify the higher fanout.

This is not a final normative disposition. It is the evidence-backed recommendation for the next Review decision.

## CI assertions

`tools/experiment_exp0003_upper_bound_routing.py` runs in the normal experiment block and requires:

- zero insertion-routing mismatches;
- zero end-to-end lookup-result mismatches on the valid stress corpus;
- zero selected-child mismatches for queries actually inside child ranges;
- every inter-child gap absence to be a full-range parent shortcut and an upper-bound one-child-read exposure;
- both strict validators to accept every generated valid parent;
- full-range parent metadata to reject every injected overlap locally;
- upper-bound parent metadata to miss those hidden-minimum overlaps locally;
- strict upper-bound validation to reject every injected overlap after child-header authentication;
- compact geometry calculations to remain reproducible.

## Boundary

This experiment does **not**:

- change `UCOF-EXP-0003.md`;
- change ObjectId width;
- change the Rust research microformat;
- change current deletion-policy bytes;
- accept FCP-0003;
- allocate EXP-0003;
- regenerate authoritative vectors.

The next Review step is to decide whether EXP-0003 keeps the explicit full-range child reference. That decision should happen before the compact-header/ObjectId-width amendment and before authoritative structural vectors are generated.

## Reproduction

```console
python3 tools/experiment_exp0003_upper_bound_routing.py
```
