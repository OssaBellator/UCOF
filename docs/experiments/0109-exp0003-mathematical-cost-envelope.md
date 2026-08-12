# Experiment 0109: EXP-0003 mathematical cost envelope

- **Status:** Reproducible analytical evidence
- **Date:** 2026-08-13
- **Related:** FCP-0003, issues #10, #13, #16, #76, Experiments 0107 and 0108
- **Script:** `tools/experiment_exp0003_cost_envelope.py`

## Question

Can EXP-0003 page-size and geometry review use a model that is less brittle than assuming every B+tree page is maximally packed and that every byte transfer has the same cost?

This experiment combines four ideas:

1. occupancy sensitivity rather than one fixed fill assumption;
2. immutable B+tree page geometry;
3. a steady-growth approximation for split-driven copy-on-write amplification; and
4. a latency/bandwidth-product model for serial authenticated path reads.

It is an analytical envelope, not a storage-provider benchmark and not a proof of UCOF's exact steady-state workload.

## Mathematical basis

### B-tree occupancy is a distribution, not a constant

Bayer and McCreight's original B-tree construction guarantees at least half occupancy for non-root nodes but explicitly observes that utilization is generally higher:

- Rudolf Bayer and Edward M. McCreight, *Organization and Maintenance of Large Ordered Indexes*, Acta Informatica 1, 173–189 (1972), DOI `10.1007/BF00288683`.

Classical random-insertion analysis associated with Yao gives approximately 69% utilization for evenly split B-trees. A recent generalization treats batched insertion workloads rather than assuming every insertion is isolated:

- Michael A. Bender et al., *Bounding the Fragmentation of B-Trees Subject to Batched Insertions*, arXiv:2603.12211 (2026), `https://arxiv.org/abs/2603.12211`.

The recent paper is especially relevant to UCOF because canonical mixed writers naturally create batches. It does **not** imply that UCOF will have exactly 69% occupancy; it just makes a 50%/69%/80%/100% sensitivity envelope more informative than a packed-only estimate.

Delete-heavy workloads deserve separate treatment. Johnson and Shasha analytically showed that insertion/deletion policy can materially change utilization and restructuring rate, and that merge-at-half can restructure much more frequently than a free-at-empty policy:

- Theodore Johnson and Dennis Shasha, *B-trees with inserts and deletes: Why free-at-empty is better than merge-at-half*, Journal of Computer and System Sciences 47(1), 45–76 (1993), DOI `10.1016/0022-0000(93)90020-W`.

That result does not directly choose UCOF's policy because immutable copy-on-write pages have different security and canonicalization constraints. It does mean that #16 should evaluate **restructuring/write amplification**, not only occupancy.

### External-memory analysis counts transfers

Aggarwal and Vitter formalized external-memory computation around transfers of contiguous blocks between slow and fast memory:

- Alok Aggarwal and Jeffrey S. Vitter, *The Input/Output Complexity of Sorting and Related Problems*, Communications of the ACM 31(9), 1116–1127 (1988), DOI `10.1145/48529.48535`.

A root-to-leaf authenticated lookup is therefore not adequately summarized by total bytes. The number of dependent page transfers matters too.

### Latency and bandwidth are separate costs

The LogP model makes communication latency and bandwidth separate parameters rather than collapsing communication into one operation count:

- David Culler et al., *LogP: Towards a Realistic Model of Parallel Computation*, UCB/CSD-92-713 (1992), `https://www2.eecs.berkeley.edu/Pubs/TechRpts/1992/6262.html`.

For UCOF's serial root-to-leaf directory path, use the simple affine transfer model

```text
T = R * L + X / W
```

where:

- `R` is the number of dependent page reads;
- `L` is fixed request/service latency;
- `X` is transferred page bytes; and
- `W` is sustained transfer bandwidth.

Multiplying by `W` gives

```text
T * W = X + R * (L * W)
```

so page-size crossover depends on the **latency-bandwidth product** `L*W`, measured in bytes.

This is useful because it separates a format decision from any one provider benchmark.

## Geometry inputs

The script evaluates both currently relevant 128-bit layouts:

| Geometry | Page header | Leaf locator | Internal reference |
|---|---:|---:|---:|
| first Draft | 80 | 64 | 72 |
| compact 128-bit evidence | 64 | 64 | 72 |

Candidate pages are 4 KiB, 16 KiB, and 64 KiB.

Occupancy scenarios are:

- exact half-full minimum implied by each capacity;
- `ln(2) ~= 0.693147` as the classical random-insert reference;
- 80%; and
- packed.

For the non-minimum scenarios, the model applies the same fill factor to internal pages. That is a **sensitivity assumption**, not a claim that Yao's leaf result is an exact internal-node theorem for EXP-0003.

## Steady-growth split approximation

Suppose the effective leaf occupancy is `E_leaf` entries/page and effective internal occupancy is `E_internal` children/page.

Asymptotically:

```text
leaf_pages(N)        ~= N / E_leaf
level_1_pages(N)     ~= N / (E_leaf * E_internal)
level_2_pages(N)     ~= N / (E_leaf * E_internal^2)
...
```

While tree height is unchanged, the derivative of one level's page population with respect to `N` is its long-run net page-growth rate. Since one split increases page population by one, the model uses

```text
leaf_split_rate             ~= 1 / E_leaf
internal_split_rate(level1) ~= 1 / (E_leaf * E_internal)
...
```

as a first-order split-event rate per insertion.

For immutable copy-on-write insertion, one page is already rewritten on each root-to-leaf level. A split at one level adds one more emitted page beyond that ordinary replacement. Therefore

```text
expected_COW_pages_per_insert
    ~= tree_levels + sum(non_root_level_split_rates)
```

Discrete root-height growth is intentionally excluded from this smooth approximation.

## Results: 100 million objects, compact 128-bit geometry, ln(2) occupancy

| Page size | Tree levels | Directory bytes | Path reads | Path bytes | Expected COW page bytes / point insert |
|---:|---:|---:|---:|---:|---:|
| 4 KiB | 6 | 9,627,860,992 | 6 | 24,576 | ~24,672 |
| 16 KiB | 4 | 9,329,049,600 | 4 | 65,536 | ~65,629 |
| 64 KiB | 3 | 9,257,025,536 | 3 | 196,608 | ~196,701 |

The important observation is that directory-space savings flatten quickly while point-update byte amplification grows almost linearly with page size.

The difference between 16 KiB and 64 KiB directory space is only about 72 MB at this scale in this occupancy regime, while a no-root-growth point insertion emits roughly three times as many page bytes at 64 KiB.

## Latency-bandwidth crossover

At the same 100-million-object / compact-128 / `ln(2)` point:

### 4 KiB versus 16 KiB

```text
4 KiB:   6 requests,  24 KiB transferred
16 KiB:  4 requests,  64 KiB transferred
```

The two costs are equal when

```text
L * W = (64 KiB - 24 KiB) / (6 - 4)
      = 20 KiB
```

Below a 20 KiB latency-bandwidth product, 4 KiB transfers win this simplified lookup model. Above it, saving two serial requests makes 16 KiB faster.

### 16 KiB versus 64 KiB

```text
16 KiB: 4 requests,  64 KiB transferred
64 KiB: 3 requests, 192 KiB transferred
```

The crossover is

```text
L * W = (192 KiB - 64 KiB) / (4 - 3)
      = 128 KiB
```

Below a 128 KiB latency-bandwidth product, 16 KiB wins the simplified point-read model. Above it, the one fewer serial range request makes 64 KiB win despite transferring three times as many directory bytes on the path.

This is the strongest reason so far **not** to freeze 16 KiB from metadata size alone.

## Implication for FCP-0003

Page size is now a multi-objective decision:

- smaller pages reduce persistent update/write amplification and path bytes;
- larger pages reduce serial dependent request count;
- total directory bytes differ much less than either effect once locators dominate metadata;
- occupancy/workload assumptions move level boundaries and therefore create discontinuities in lookup cost.

Therefore the next page-size decision should be based on a representative backend/workload envelope, not a single packed-tree arithmetic row.

A useful acceptance table should include at least:

1. local buffered file;
2. local direct/random I/O if supported;
3. maintained HTTP range source;
4. versioned cloud-object source;
5. point lookup;
6. canonical bulk build;
7. persistent point update;
8. mixed/batched update;
9. compaction/rewrite.

Issue #10 should provide measured `L` and `W` (and eventually tail distributions), after which this model can map those measurements onto 4/16/64 KiB candidates without changing the model itself.

## New concern for issue #16

The Johnson–Shasha result makes one specific follow-up important for immutable UCOF:

> compare strict merge-at-half occupancy against its page-rewrite/restructuring cost under insert/delete mixtures.

UCOF's current half-full invariant may still be the right choice for canonicality and bounded worst-case shape. But copy-on-write makes every borrow/merge physically expensive, so occupancy percentage alone is not a sufficient objective.

A follow-up model should track at least:

```text
occupancy distribution
borrow rate
merge rate
split rate
pages emitted per operation
bytes emitted per operation
history bytes retained
```

under controlled insertion/deletion/batch distributions.

## Further mathematics worth applying

The following techniques appear directly useful to later EXP-0003 decisions:

- **renewal / Markov-chain occupancy models** for split/borrow/merge cycles;
- **mean-field or fringe analysis** for large-tree occupancy distributions;
- **robust optimization / Pareto frontiers** for page size across conflicting read/write/backend objectives;
- **queueing and tail-distribution models** once real provider latency samples exist;
- **exact birthday bounds plus namespace-generation models** for 64/128-bit `ObjectId` policy;
- **reliability / hazard models** for staged publication and restart evidence once #11 reaches physical fault qualification.

The goal is not mathematical decoration. Each model should exist only when it changes a concrete format, implementation, or qualification decision.

## Reproduction

```console
python3 tools/experiment_exp0003_cost_envelope.py
python3 tools/experiment_exp0003_cost_envelope.py --json
```

The script contains deterministic self-checks for the compact-128 100-million-object `ln(2)` case, including the 20 KiB and 128 KiB latency-bandwidth crossover products.
