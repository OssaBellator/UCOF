# Experiment 0134 — EXP-0003 source deletion-policy information cost

**Status:** non-normative research evidence  
**Date:** 2026-08-13  
**Related:** Experiments 0112, 0119, 0121–0123, 0128–0133; issues #10, #13, #16, #76

## Question

The accumulated deletion-policy evidence shows that `FullerSiblingLeftTie` can reduce future expensive-state visitation and immutable page writes relative to the current `LeftFirst` rule. That does not make the alternative free.

The in-memory/slice persistent writer already loads both immediate siblings before borrower selection, but the current strongly-versioned **source-backed deletion planner** can short-circuit:

1. load the left sibling;
2. if it can lend, borrow from it and return;
3. load the right sibling only when the left sibling cannot lend.

Therefore a fuller-sibling rule can require additional authenticated source information in exactly the two-donor cases where current LeftFirst would stop after the left sibling.

This experiment measures that marginal information cost before adding any policy-aware source-planning API.

## Fixture

The executable recreates the same deterministic candidate-review fixture used by Experiment 0121:

```text
genesis objects: 1..=370
insert:          371..=379
delete:          1..=91 under LeftFirst
target delete:   ObjectId 186
```

The resulting active leaf occupancies around the target are:

```text
left   = 94
target = 93
right  = 101
```

The read-only frontier inspector requires:

```text
LeftFirst             -> left donor 94
FullerSiblingLeftTie  -> right donor 101
```

The owned persistent writers are also checked:

- both policies write 3 pages for this deletion;
- their persistent bytes differ;
- the current source planner must reproduce the owned LeftFirst append tail exactly.

## Bounded source contract

The experiment uses the same deliberately small request cap as the existing source-planning stress tests:

```text
PAGE_SIZE              = 16,384 bytes
max_read_request_bytes = 257 bytes
```

After the current LeftFirst source plan succeeds, the experiment independently parses the authenticated active root, identifies the right-sibling page reference, reads that exact page through a strongly-versioned bounded source, verifies the page SHA-256 against the parent reference, and requires occupancy `101`.

No Fuller source-planning API is introduced. The probe measures only the extra information needed to compare both eligible donor occupancies under the current architecture.

## CI-reproduced result

The semantic head reproduced:

| Metric | Current LeftFirst source plan | Additional right-sibling information | Marginal share of current plan |
|---|---:|---:|---:|
| Read operations | 15,096 | 64 | 0.4240% |
| Bytes read | 3,661,003 | 16,384 | 0.4475% |
| Bytes hashed | 3,660,235 | 16,384 | 0.4476% |
| Strong-version checks | 30,202 | 128 | 0.4238% |
| Request cap | 257 bytes | same | — |
| Right-sibling occupancy authenticated | — | 101 | — |
| Immediate pages written | 3 | Fuller also 3 | equal |

The exact executable output is:

```text
metric,left_first_source,fuller_information_delta
read_operations,15096,64
bytes_read,3661003,16384
bytes_hashed,3660235,16384
version_checks,30202,128
request_cap_bytes,257,0
right_sibling_occupancy,0,101
left_pages_written,3,0
fuller_pages_written,3,0
persistent_outputs_equal,0,0
```

## Real trace opportunity frequency

The existing five-workload Rust trace matrix now records a separate `source_info` row without changing the pinned reward histogram.

A source-information opportunity is counted on an underflow when:

```text
left sibling exists and occupancy > M
right sibling exists
```

At that point the current LeftFirst source planner can authenticate the lendable left page and stop, while a fuller-sibling comparison also needs authenticated right-sibling occupancy unless that information is already cached.

CI reproduced:

| Trace | LeftFirst trajectory opportunities | Fuller trajectory opportunities |
|---|---:|---:|
| whole-set LCG | 8 | 1 |
| left-leaf hot | 0 | 0 |
| middle-leaf hot | 1 | 1 |
| right-leaf hot | 0 | 0 |
| left/middle boundary hot | 12 | 1 |
| **Total** | **21** | **3** |

The LeftFirst trajectory therefore exposes the extra-information condition on:

```text
21 / 240 deletions = 8.75%
```

of these structured deletion traces.

In this particular matrix, the 21 LeftFirst information opportunities coincide exactly with the 21 locally avoidable donor-cliff events already recorded by Experiment 0128. That equality is a property of these traces, not a general identity: another state can have a lendable left sibling and present right sibling without the left donor being exactly `M+1`.

If every one of those 21 LeftFirst-trajectory opportunities were evaluated with the same uncached 257-byte-cap right-page probe, the **one-step exposure** would be:

```text
21 * 64 reads          = 1,344 bounded reads
21 * 16,384 bytes      = 344,064 bytes read
21 * 16,384 bytes      = 344,064 bytes hashed
21 * 128 version checks = 2,688 strong-version checks
```

This is not an actual Fuller trajectory cost. The policies produce different persistent states after a donor-choice divergence, so later opportunities are endogenous to the selected policy. The table should be read as the information exposure present on each policy's own measured trajectory, not as an additive counterfactual bill obtained by multiplying LeftFirst events after switching policies.

For context, the same five persistent traces measured a 38-page write difference, or 622,592 current-microformat page bytes, in favor of Fuller. The `344,064` read-byte exposure and `622,592` write-byte saving must **not** be subtracted as if bytes had one universal price: request latency, hashing, transfer pricing, caching, append/durability cost, and the policy-dependent trajectory all differ. They are useful as two separately measured physical quantities for a later transport-aware cost model.

## General bounded-read formula

For a page of size `P` and a source request cap `B`, if the right sibling was not already cached and must be authenticated after LeftFirst could have stopped, the current read/check pattern costs:

```text
extra read operations   = ceil(P / B)
extra bytes read        = P
extra bytes hashed      = P
extra version checks    = 2 * ceil(P / B)
```

For this experiment:

```text
ceil(16,384 / 257) = 64
```

so the measured `64 / 16,384 / 16,384 / 128` marginal accounting follows directly from the bounded source contract.

If a future maintained adapter permits one complete 16 KiB page per source request, the corresponding planning information requirement would be one additional page read rather than 64 bounded `read_exact_at` calls. Conversely, a transport implementation may map one logical bounded read to more than one physical/network action. The experiment does not equate `read_exact_at` calls with provider billing or network round trips.

## Why the marginal percentage is small here

The current source deletion planner performs strict source validation before the path-local planning pass. That validation dominates the measured totals, and its previously read page bytes are not retained as a planning cache.

Therefore the ~0.42–0.45% marginal percentages are properties of this current research API and this fixture. They are **not** a general statement that fuller-sibling costs less than half a percent on HTTP/cloud storage.

The important implementation fact is narrower:

> Under the current planner, a two-donor LeftFirst short-circuit can avoid one sibling-page authentication that a fuller-sibling comparison needs unless that occupancy is already available or cached.

## Relation to the write-side evidence

Experiment 0121 shows that on this exact local fixture both policies have equal immediate write reward while producing different persistent identities. Experiments 0119/0123/0128 show the longer-run benefit comes from changed state visitation rather than an intrinsically cheaper borrow operation.

Experiment 0134 supplies the corresponding information-side cost:

```text
Fuller decision
  may require one additional sibling-page authentication now
  in exchange for avoiding some donor-cliff states
  that can reduce future expensive repair-state visitation.
```

This is the correct trade-off surface for the FCP decision. Neither "fewer future page writes" nor "one extra sibling read" should be evaluated in isolation.

## Decision-model form

A transport-aware comparison can express the incremental fuller-sibling decision cost as:

```text
Delta C
  = p_extra_sibling * C_read_page
    - expected_future_pages_saved * C_write_page
```

where:

- `p_extra_sibling` is the workload probability that the policy needs a sibling page LeftFirst would not have read/cached;
- `C_read_page` includes request latency, transferred bytes, hashing, and version checks under the actual source adapter;
- `expected_future_pages_saved` comes from policy-dependent state visitation, not the immediate borrow reward;
- `C_write_page` includes append/persistence cost for the deployment.

The repository does not yet have sufficiently qualified provider measurements to assign one universal value to those costs. Issue #10 remains the correct place for maintained HTTP/cloud measurements.

## Boundary

This experiment does **not**:

- add a Fuller source planner;
- change the default `LeftFirst` rule;
- claim 64 network round trips;
- claim the measured percentage applies to providers;
- treat LeftFirst-trajectory opportunity counts as an actual Fuller counterfactual trajectory;
- accept FCP-0003;
- change page/entry geometry;
- change authoritative vectors;
- allocate EXP-0003.

It closes the main practical information-cost gap in the deletion-policy evidence package. The next artifact should be a bounded deletion-policy decision packet for #13/#16 rather than another open-ended policy simulation.

## Reproduction

```console
cargo run --locked -p ucof-experiments --example exp0003_source_delete_policy_io
cargo run --locked -p ucof-experiments --example exp0003_delete_policy_trace_matrix
```

The normal Rust CI job runs these executables alongside the pinned candidate-vector, reward-decomposition, and deletion-parent-reward checks.
