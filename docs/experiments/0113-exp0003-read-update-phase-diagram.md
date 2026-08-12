# Experiment 0113: EXP-0003 read/update page-size phase diagram

- **Status:** Reproducible analytical evidence
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #10, Experiments 0107 and 0109
- **Script:** `tools/experiment_exp0003_workload_phase.py`

## Question

Experiment 0109 showed that page-size lookup crossovers depend on the latency-bandwidth product

```text
Q = L * W
```

where `L` is fixed read request latency and `W` is sustained read bandwidth.

But UCOF is not read-only. Immutable point updates rewrite one page on each root-to-leaf level and may emit extra pages on splits. How does update pressure move the 4/16/64 KiB page-size crossover?

## A dimensionless two-objective model

For page-size candidate `j`, Experiment 0109 supplies:

```text
R_j = serial authenticated path reads
X_j = authenticated path bytes
Y_j = expected COW page bytes for a point insertion
```

A read lookup takes the simple affine time model

```text
T_read,j = R_j * L + X_j / W
```

Multiplying by read bandwidth gives a byte-equivalent cost

```text
W * T_read,j = X_j + R_j * Q
```

with

```text
Q = L * W
```

Now define an **effective update pressure** `U >= 0`.

`U` absorbs both:

- updates per lookup; and
- the relative cost of writing one byte versus transferring one read byte.

The combined normalized objective is

```text
C_j(Q, U) = X_j + R_j * Q + U * Y_j
```

`U` is intentionally dimensionless. It is not yet a storage-provider measurement. It is a way to expose the geometry of the design decision before #10 supplies real backend parameters.

This model also intentionally omits publication/fsync/PUT request latency. Those can later be added as separate affine terms if measurements show they matter to the page-size decision.

## Pairwise phase boundaries are straight lines

For page sizes `a` and `b`, the equality boundary is

```text
X_a + R_a Q + U Y_a
    = X_b + R_b Q + U Y_b
```

so

```text
Q(U)
  = ((X_b - X_a) + U (Y_b - Y_a)) / (R_a - R_b)
```

provided `R_a != R_b`.

Therefore every pairwise page-size crossover is an affine line in the `(U, Q)` plane.

The optimum among 4/16/64 KiB is the lower envelope of three affine cost surfaces.

That is a more useful object than asking for one globally optimal page size.

## Reference geometry

The clearest reference point remains Experiment 0109's:

```text
geometry      = compact 128-bit
objects       = 100,000,000
occupancy     = ln(2) random-insert sensitivity regime
```

The inputs are approximately:

| Page | Path reads `R` | Path bytes `X` | Expected COW bytes `Y` |
|---:|---:|---:|---:|
| 4 KiB | 6 | 24,576 | 24,672.278 |
| 16 KiB | 4 | 65,536 | 65,629.290 |
| 64 KiB | 3 | 196,608 | 196,700.569 |

## Boundary equations

### 4 KiB vs 16 KiB

```text
Q(U) ~= 20,480 + 20,478.506 U bytes
```

At `U=0`, this reduces to Experiment 0109's 20 KiB read-only crossover.

Every unit of effective update pressure raises the latency-bandwidth product needed to justify 16 KiB by another approximately 20 KiB.

### 16 KiB vs 64 KiB

```text
Q(U) ~= 131,072 + 131,071.279 U bytes
```

At `U=0`, this is the 128 KiB read-only crossover.

Because 64 KiB also emits roughly 131 KiB more COW bytes per modeled point insertion than 16 KiB, update pressure moves this boundary rapidly upward.

### 4 KiB vs 64 KiB

```text
Q(U) ~= 57,344 + 57,342.764 U bytes
```

This pairwise crossing lies behind the 16 KiB lower-envelope region for the reference case, but remains useful as a consistency check.

## Lower-envelope regions

For the reference case, the preferred page size under this objective is:

### Read-only (`U = 0`)

```text
Q < 20,480                 -> 4 KiB
20,480 <= Q < 131,072      -> 16 KiB
Q >= 131,072               -> 64 KiB
```

### Light update pressure (`U = 0.1`)

```text
Q < 22,527.85              -> 4 KiB
22,527.85 <= Q < 144,179.13 -> 16 KiB
Q >= 144,179.13            -> 64 KiB
```

### Equal normalized update/read pressure (`U = 1`)

```text
Q < 40,958.51               -> 4 KiB
40,958.51 <= Q < 262,143.28 -> 16 KiB
Q >= 262,143.28             -> 64 KiB
```

### Strong update pressure (`U = 10`)

```text
Q < 225,265.06                -> 4 KiB
225,265.06 <= Q < 1,441,784.79 -> 16 KiB
Q >= 1,441,784.79              -> 64 KiB
```

The qualitative result is simple:

> write/update pressure expands the region where smaller pages are optimal; increasingly high latency-bandwidth product is required to compensate for large-page copy-on-write amplification.

## Why this model is better than a weighted score table

A fixed weighted score hides the assumptions in one number.

The phase diagram instead exposes the decision boundary itself. Maintainers can later plug in different backend/workload measurements and see whether they fall inside the same page-size region.

It also makes robustness visible:

- a workload far from a boundary supports a stable decision;
- a workload near a boundary says the format choice is sensitive to measurement error or deployment differences;
- if realistic deployments occupy multiple regions, a single global page size is imposing a genuine trade-off rather than approximating a universal optimum.

## Connection to Pareto analysis

The page candidates trade at least three resources:

```text
serial read requests
read bytes
immutable write bytes
```

None of 4/16/64 KiB universally dominates the others in the reference regime:

- 4 KiB minimizes read and write bytes but maximizes request count;
- 64 KiB minimizes request count but maximizes read/write bytes;
- 16 KiB is intermediate.

The phase diagram is therefore a scalarized view of a Pareto frontier, with `(Q,U)` making the scalarization assumptions explicit.

## What #10 should measure

To turn the phase diagram into deployment evidence, maintained local/HTTP/cloud adapters should report at least:

```text
read fixed-latency distribution
read sustained bandwidth
range-size-dependent latency
cache hit/miss regime
write bandwidth or byte cost
publication/request latency
point lookup frequency
point update frequency
batch-size distribution
```

A first backend estimate can map to

```text
Q = L * W
U = (updates/lookups) * relative_write_byte_cost
```

but tail latency and non-linear range pricing should remain visible rather than being averaged away if they change the winning region.

## Batched-update correction is still needed

`Y_j` currently comes from Experiment 0109's point-insert split-rate approximation.

UCOF canonical mixed writers naturally issue batches, and recent work by Bender et al. generalizes classical B-tree fragmentation analysis to batched consecutive-key insertions. That means the next refinement should replace one point-insert `Y_j` with a family

```text
Y_j(batch_length, locality_model)
```

rather than assuming every update is independent.

The useful design question becomes:

> do realistic UCOF batch distributions move deployments across a page-size phase boundary?

If not, exact batch modeling is unlikely to affect the byte-layout decision. If they do, batching must be part of the FCP evidence.

## Limitations

This experiment does not model:

- deletion borrow/merge amplification;
- full mixed-operation stationary occupancy;
- write request latency or fsync/publication barriers;
- provider request pricing;
- prefetch/concurrent range reads;
- page cache effects;
- compaction or bulk build;
- failure/retry cost; or
- tail-latency objectives.

Those should be added only when they can change the format decision.

## Reproduction

```console
python3 tools/experiment_exp0003_workload_phase.py
python3 tools/experiment_exp0003_workload_phase.py --json
```

The script derives phase boundaries from Experiment 0109 rather than duplicating tree geometry, and includes deterministic checks for the 100-million-object compact-128 `ln(2)` reference case.
