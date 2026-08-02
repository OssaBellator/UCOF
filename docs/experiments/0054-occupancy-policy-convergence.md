# Experiment 0054 — Occupancy-policy convergence

**Status:** Design blocker recorded  
**Scope:** Current immutable-successor microformat versus FCP-0003 Draft

## Question

Can the proposed half-full non-root occupancy, deterministic deletion repair, and current reusable writer bytes coexist without changing canonical genesis and append identities?

## Current executable behavior

The current research writer builds leaves by taking consecutive `LEAF_CAPACITY` chunks and then builds internal pages the same way. It does not redistribute the final two pages to satisfy a minimum occupancy.

With the current 16 KiB page and 88-byte research leaf entry:

- `LEAF_CAPACITY = 185`;
- a 400-object genesis is packed as `185, 185, 30` leaf entries;
- the final non-root leaf therefore contains 30 entries.

A half-full rule rounded up would require at least 93 entries in every non-root leaf. The stored and recipe-pinned 400-object research vector is valid under the current microformat but would not conform to that proposed rule.

The same issue exists for internal construction: the final internal page may contain fewer than half of `INTERNAL_FANOUT` children when a deeper tree is built by simple maximum-sized chunks.

## Consequence

Adopting the FCP-0003 occupancy proposal changes deterministic tree shape, page bytes, page digests, snapshot identity, commit identity, vector lengths, and large-tree transition recipes. This is not an implementation-only deletion optimization.

A deletion writer cannot honestly claim the proposed borrow/merge invariant while accepting arbitrary current research trees unless it first normalizes affected sparse boundaries or performs a full canonical rebuild. Conversely, implementing deletion with a minimum occupancy of one would advance current microformat code but would not implement the proposed epoch policy.

## Options

### A. Keep maximum packing with a sparse final page

- Retains current research vector identities.
- Requires deletion policy that permits sparse boundary pages.
- Produces weaker worst-case occupancy and more policy exceptions.
- Conflicts with the current FCP-0003 half-full wording.

### B. Redistribute the final two pages during canonical construction

- Satisfies half-full occupancy for non-root pages.
- Changes all vectors whose final page is below minimum occupancy.
- Gives insertion and deletion one consistent invariant.
- Requires new cross-language genesis, append, split, redistribution, merge, and root-collapse vectors.

### C. Allow a designated sparse rightmost boundary page

- Preserves maximum packing for most pages.
- Adds a special case to lookup, insertion, deletion, validation, and conformance.
- Requires precise rules when the sparse boundary moves or merges.
- Risks reproducing complexity the half-full rule was intended to avoid.

### D. Defer minimum occupancy to implementation policy

- Keeps byte validation simpler.
- Permits independently valid writers to create materially different canonical trees unless another tree-shape rule is normative.
- Undermines deterministic page identity and cross-writer byte equality.

## Recommendation

For a new disposable epoch, select **Option B**: canonical final-two-page redistribution with half-full non-root occupancy. Treat every current immutable-successor byte identity as non-epoch research evidence and regenerate transition vectors under the accepted policy.

Until that proposal is accepted and implemented, persistent deletion should be split into two clearly labelled layers:

1. a current-microformat experiment that does not claim FCP-0003 occupancy conformance; or
2. an epoch-convergence writer branch that first changes canonical construction and all affected vectors.

The second path is preferable for normative progress. The first remains useful only for algorithm and hostile-input evidence.

## Required convergence work

- amend the successor specification with exact minimum occupancy and final-two-page redistribution rules;
- update genesis and full-rebuild construction in Rust and Python;
- regenerate and pin every changed vector identity;
- add limit and canonicality rejection cases for underfull non-root pages;
- integrate persistent insertion and deletion against the same invariant;
- define whether legacy non-epoch research bytes are accepted only by research tools or retired entirely;
- obtain independent reproduction of tree shape for boundary counts around every leaf and internal capacity.

## Boundary counts to pin

At minimum, generate exact trees for:

- `1`, `LEAF_CAPACITY - 1`, `LEAF_CAPACITY`, `LEAF_CAPACITY + 1`;
- `2 * LEAF_CAPACITY - 1`, `2 * LEAF_CAPACITY`, `2 * LEAF_CAPACITY + 1`;
- counts producing a final leaf at minimum minus one, exact minimum, and minimum plus one;
- `LEAF_CAPACITY * INTERNAL_FANOUT` and adjacent counts;
- counts producing a final internal page at minimum minus one, exact minimum, and minimum plus one.

Technical validation of current bytes does not resolve this policy choice.