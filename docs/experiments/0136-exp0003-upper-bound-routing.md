# Experiment 0136 — EXP-0003 upper-bound-only internal routing

**Status:** non-normative research evidence  
**Date:** 2026-08-13  
**Related:** Experiments 0108, 0135; FCP-0003; issues #13, #16, #76

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

The Draft's insertion routing rule is already expressed primarily in terms of child maxima: an ID in a sparse gap routes to the first child whose maximum is greater than the ID, and an ID beyond every maximum routes to the final child.

This experiment asks whether the duplicated child minimum can be removed from the internal entry without changing valid-tree routing semantics, and what that buys under the compact 128-bit and 64-bit geometries considered by Experiments 0108 and 0135.

## Candidate internal reference

For an identifier width `I`, the upper-bound-only entry is:

```text
child maximum ObjectId   I bytes
child page offset        8 bytes
child page digest       32 bytes
```

Thus:

| Identifier width | Current/full-range compact entry | Upper-bound-only entry |
|---:|---:|---:|
| 128 bits | 72 bytes | 56 bytes |
| 64 bits | 56 bytes | 48 bytes |

The candidate still authenticates the exact child page bytes through the child digest. It removes only the duplicated child minimum from the parent entry.

## Routing interpretation

Let the ordered child maxima stored in one parent be:

```text
U0 < U1 < ... < Un
```

For lookup or insertion of key `q`, choose the first child whose upper bound satisfies:

```text
q <= Ui
```

For insertion only, if `q > Un`, choose the final child.

This makes the parent-authenticated routing intervals implicit:

```text
first child:              q <= U0
child i, i > 0:    U(i-1) < q <= Ui
```

The child's *actual* minimum may be greater than the lower routing boundary. That is how sparse gaps remain representable: a key in the gap routes to the next child and is then absent from that child.

The stored `Ui` is still required to equal the authenticated child page's actual maximum during strict validation.

## Valid-tree routing equivalence

For a valid ordered set of child ranges:

```text
[min0, max0], [min1, max1], ...
```

with:

```text
min(i) > max(i-1)
```

the first child selected by the Draft's current rules is exactly the first child with:

```text
max(i) >= q
```

when such a child exists.

That covers all cases:

- `q` lies inside a child range: that child's maximum is the first maximum at or above `q`;
- `q` lies in a sparse gap: the next child's maximum is the first maximum at or above `q`;
- `q` lies before the first actual minimum: the first child is selected;
- `q` exceeds every maximum: lookup can return out-of-parent-range absence, while insertion selects the final child exactly as the current Draft requires.

The executable generated **992** valid sparse parents with 2–32 children and exhaustively checked **170,947** integer query positions spanning the generated parent ranges and surrounding gaps.

Result:

```text
lookup routing mismatches:    0
insertion routing mismatches: 0
```

The integer model is only a compact test domain. The proof depends solely on total ordering, so it applies equally to lexicographically ordered fixed-width opaque `ObjectId` bytes.

## Strict recursive validation

The current full-range entry lets a validator check declared sibling non-overlap from the parent page alone before opening children.

With upper-bound-only entries, strict validation instead reconstructs the omitted facts while recursively authenticating child pages:

1. parent maxima are non-zero and strictly increasing;
2. each stored maximum equals the authenticated child header maximum;
3. first child minimum equals parent minimum;
4. for each later child:

   ```text
   child_min > previous_stored_child_max
   ```

5. final stored child maximum equals parent maximum;
6. expected level, page digest, occupancy, and all ordinary page checks still apply.

This preserves the same final strict non-overlap invariant because Section 21 of the Draft already requires recursive authentication of every reachable directory page.

The executable injected **992** overlapping-child cases while leaving stored maxima strictly increasing.

Results:

| Check | Full-range parent | Upper-bound parent |
|---|---:|---:|
| Detect overlap from parent metadata alone | 992 / 992 | 0 / 992 |
| Detect overlap after strict child-header authentication | 992 / 992 | 992 / 992 |

So the candidate does **not** preserve the same *early parent-local rejection point*. It does preserve full strict-validation detection once the child header is authenticated.

That distinction is material and should remain visible in Review.

## Targeted lookup / absence boundary

Section 22 deliberately gives targeted lookup a narrower assurance scope than full validation: it authenticates one root-to-leaf path rather than recursively opening unrelated siblings.

Upper-bound-only routing can still provide a deterministic authenticated routing interval from the parent maxima, but it cannot independently verify the omitted actual minima of unopened sibling pages.

The current full-range design is stronger at the parent-metadata layer because it carries explicit declared minima and maxima for every sibling. Even there, a one-path lookup does not authenticate unopened sibling page bodies, so it also does not independently cross-check every declared sibling bound against the corresponding child header.

Therefore the normative question is not simply "does max-only routing work?" It does on valid trees. The real question is what Section 22 means by an authenticated absence result on input that has **not** already passed complete strict validation.

A safe adoption needs one of these explicit contracts:

1. **Routing-interval contract:** parent maxima define the authenticated routing partition used by targeted lookup, while targeted mode does not claim it verified unopened child-header conformance. Complete strict validation remains the mode that proves every child fits that partition.
2. **Prior-validation/cache contract:** targeted absence is offered only against a tree/prefix whose structural validity has already been established and retained under a suitable trust/cache contract.
3. **Extra-evidence contract:** targeted lookup authenticates additional sibling/header evidence sufficient for the stronger absence claim.

The first option is the smallest wire design, but it must be stated rather than inferred.

## Geometry

The experiment keeps 16 KiB pages and the compact header candidates from Experiment 0108.

### Compact 128-bit

```text
page header: 64 bytes
leaf entry:  64 bytes
leaf cap:   255
```

Internal geometry:

```text
full-range entry: 72 bytes -> fanout 226
upper-bound entry: 56 bytes -> fanout 291
```

### Compact 64-bit

```text
page header: 48 bytes
leaf entry:  56 bytes
leaf cap:   291
```

Internal geometry:

```text
full-range entry: 56 bytes -> fanout 291
upper-bound entry: 48 bytes -> fanout 340
```

## Directory-byte effect

Because leaf pages dominate the primary directory, reducing only internal-reference width has a modest steady-state byte effect at scales where tree height does not change.

| Layout | Objects | Full-range directory bytes | Upper-bound directory bytes | Saving | Saving % |
|---|---:|---:|---:|---:|---:|
| compact-128 | 10M | 645,382,144 | 644,743,168 | 638,976 | 0.0990% |
| compact-128 | 100M | 6,453,690,368 | 6,447,284,224 | 6,406,144 | 0.0993% |
| compact-128 | 1B | 64,536,576,000 | 64,472,580,096 | 63,995,904 | 0.0992% |
| compact-64 | 10M | 565,002,240 | 564,723,712 | 278,528 | 0.0493% |
| compact-64 | 100M | 5,649,694,720 | 5,646,876,672 | 2,818,048 | 0.0499% |
| compact-64 | 1B | 56,496,603,136 | 56,468,537,344 | 28,065,792 | 0.0497% |

This is useful but not a reason by itself to complicate targeted semantics.

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

At 10 billion objects in this simple fully packed geometry model:

```text
compact-128 full-range:  5 levels
compact-128 upper-bound: 5 levels
compact-64 full-range:   5 levels
compact-64 upper-bound:  4 levels
```

Thus the internal-width change can remove an entire page read/write/hash level near a scale threshold even though its ordinary directory-byte percentage is small.

## Interaction with mutation cost

Every changed immutable child already changes its digest, so the parent reference must be rewritten whether or not the child's minimum changed. Omitting child minima therefore does **not** eliminate parent rewrites for ordinary copy-on-write mutation.

The benefit is instead:

- higher internal fanout;
- fewer internal pages in bulk/rewrite form;
- higher object-count thresholds before tree height grows;
- smaller parent entry parsing/state;
- one fewer duplicated range claim per child.

The cost is weaker parent-local malformed-range rejection unless the child is opened.

## Decision consequence

The evidence supports treating upper-bound-only internal references as a serious Draft amendment candidate, but **not silently adopting them yet**.

A reasonable Review disposition is:

> Adopt upper-bound-only child references only if the targeted lookup/absence section explicitly defines parent upper bounds as authenticated routing intervals and clearly states that targeted mode does not verify unopened child-header conformance. Complete strict validation must still reconstruct child minima and reject overlap.

If Review instead wants one authenticated internal page to carry explicit declared non-overlapping child ranges without relying on implicit routing partitions, retain the 72-byte/56-byte full-range forms.

This is primarily an assurance-contract decision, not a capacity decision.

## CI assertions

`tools/experiment_exp0003_upper_bound_routing.py` runs in the normal experiment block and requires:

- zero lookup-routing mismatches on the deterministic sparse-parent corpus;
- zero insertion-routing mismatches;
- both full-range and upper-bound strict validators to accept every generated valid parent;
- full-range parent metadata to reject every injected declared overlap locally;
- upper-bound parent metadata to miss those hidden-minimum overlaps locally;
- strict upper-bound validation to reject every injected overlap after child-header authentication;
- compact geometry calculations to remain reproducible.

## Boundary

This experiment does **not**:

- change `UCOF-EXP-0003.md`;
- change ObjectId width;
- change the Rust research microformat;
- change current `LeftFirst` deletion bytes;
- accept FCP-0003;
- allocate EXP-0003;
- regenerate authoritative vectors.

The next Review step is to decide whether the targeted-absence assurance contract permits the implicit upper-bound routing partition. Only after that decision should the compact geometry and ObjectId-width recommendation be folded into the Draft.

## Reproduction

```console
python3 tools/experiment_exp0003_upper_bound_routing.py
```
