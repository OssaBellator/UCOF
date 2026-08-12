# Experiment 0111: EXP-0003 history-independent partition locality

- **Status:** Reproducible analytical/stochastic evidence
- **Date:** 2026-08-13
- **Related:** FCP-0003, issues #16 and #76, Experiments 0109 and 0110
- **Script:** `tools/experiment_exp0003_history_independent_partition.py`

## Question

Can EXP-0003 make leaf partition identity depend only on the current ordered `ObjectId` set **without** giving up the update locality and page reuse that motivated the persistent immutable B+tree?

The current Draft intentionally allows persistent roots to remain history-sensitive. Before freezing that choice, this experiment compares simple history-independent partition families that expose the mathematical trade-offs.

This is not an implementation proposal. The purpose is to identify which guarantees are simultaneously easy, which conflict, and what a stronger partition primitive would have to deliver.

## Four comparison regimes

### 1. Packed-by-rank canonical pages

Sort the active `ObjectId`s and cut every `C` entries.

For the first-Draft 16 KiB leaf geometry:

```text
C = floor((16,384 - 80) / 64) = 254
```

This is fully history-independent and maximally dense except for the final page.

But an insertion at rank `r` shifts every later page boundary. For a uniformly random edit position, roughly half the pages are expected to change. It is therefore a useful **canonicality upper bound / locality lower bound**.

### 2. Bernoulli hash anchors

Give every identifier a deterministic SHA-256 score and start a new group whenever

```text
score(ObjectId) < 2^256 / C
```

Under a random-score model, each identifier is independently an anchor with probability approximately `1/C`, so group lengths have a geometric distribution with mean approximately `C`.

This yields expected-local history-independent groups: adding or removing one identifier normally affects only the group containing that identifier, or splits/joins a nearby group if the identifier is an anchor.

However, a geometric distribution has no hard maximum. Approximately

```text
P(group length > C) ~= (1 - 1/C)^C ~= e^-1 ~= 36.8%
```

under the idealized model. An anchor-only scheme therefore cannot directly satisfy UCOF's fixed-page capacity.

### 3. Sliding-window minimizer boundaries

For every window of `w=C=254` consecutive identifiers, select the identifier with the lowest deterministic SHA-256 score, breaking equal scores to the left. The union of selected identifiers forms group boundaries.

This is the standard minimizer/winnowing construction applied to ordered identifiers rather than text k-mers.

The useful mathematical properties are:

- every window of `w` candidates contains a selected minimizer, so the gap between selected positions is at most `w`;
- identical sufficiently long regions select identical minimizers, so edits only disturb selection in a bounded neighborhood;
- under the classic random-order hypothesis, minimizer density is approximately `2/(w+1)`.

With `w=C`, the random-order mean spacing is therefore approximately

```text
(w + 1) / 2 = 127.5
```

which is almost exactly the current half-full threshold `M=127`.

That is an important structural result: **the simplest history-independent bounded-locality scheme naturally trades the current near-full packed density for roughly half-capacity mean groups**. It also has no positive minimum gap, so tiny groups remain possible.

Relevant minimizer analysis includes:

- Guillaume Marçais, Dan DeBlasio, Prashant Pandey, Carl Kingsford, *Locality-sensitive hashing for the edit distance*, and later minimizer analyses summarized in Bioinformatics literature;
- Jim Shaw and Yun William Yu, *Theory of local k-mer selection with applications to long-read alignment*, Bioinformatics 38(20), 2022;
- Guillaume Marçais et al., *Improving the performance of minimizers and winnowing schemes*, Bioinformatics 33(14), 2017.

The exact biological-string setting is different from UCOF. The window guarantee and selection-density mathematics are the relevant abstractions.

### 4. Current-style persistent split/repair round trip

As a history-sensitivity control, start from packed pages, insert one identifier into a full leaf using the current proposed

```text
255 -> 128,127
```

split, then delete that exact identifier using the current half-full left-borrow/right-borrow/left-merge/right-merge leaf rule.

The logical set returns exactly to its starting state, but the split normally remains because both resulting leaves are legal at 127 entries.

This isolates the benefit and cost of scoped determinism:

- almost all old pages remain reusable;
- equal current logical sets need not have equal leaf partitions.

## Chosen-identifier grinding control

Hash-derived partition priorities are only pseudorandom when identifiers are not selected adaptively against the public hash rule.

UCOF `ObjectId` policy is still under review. If a writer can choose identifiers, it can evaluate many legal candidate identifiers and select one with an unusually low partition score.

For each grinding trial, this experiment chooses the lowest SHA-256 score among 256 deterministic candidate identifiers within the selected key gap.

This is deliberately modest. It is not a cryptanalytic attack on SHA-256; it is ordinary input selection against a public deterministic priority function.

For a Bernoulli anchor threshold `p=1/C`, the idealized probability that at least one of `G` independent candidates is an anchor is

```text
1 - (1 - p)^G
```

so with `C=254` and `G=256` the probability is already about 63.6%.

A public hash-anchor scheme must therefore treat chosen-key grinding as part of the algorithmic threat model, not as a hash-function failure.

## Evidence configuration

The full recorded ensemble uses:

```text
seeds                    = 3, 17, 29, 43, 71
objects / seed           = 25,000
normal inserts / seed    = 100
random deletes / seed    = 100
ground inserts / seed    = 100
grind candidates         = 256
ObjectId width            = 128 bits
priority                  = SHA-256(ObjectId bytes)
page capacity             = 254 entries
half-full threshold       = 127 entries
minimizer window          = 254 entries
```

Each edit trial is independent from the same base set so the measured changed-group count is a one-operation locality metric rather than a long-history occupancy process.

## Results

| Scheme | Mean group | Mean max group | Groups below 127 | Groups above 254 | Changed groups / insert | Old keys in exactly reused groups |
|---|---:|---:|---:|---:|---:|---:|
| packed rank | 252.53 | 254.0 | 1.01% | 0% | 50.592 | 49.18% |
| hash anchor | 258.75 | 1,115.8 | 38.98% | 37.04% | 1.000 | 98.01% |
| window minimizer | 126.51 | 252.6 | 49.35% | 0% | 1.004 | 99.33% |

Random deletion shows the same locality split:

| Scheme | Changed groups / delete | Old keys in exactly reused groups |
|---|---:|---:|
| packed rank | 49.636 | 50.15% |
| hash anchor | 1.000 | 98.14% |
| window minimizer | 1.002 | 99.31% |

### Packed rank proves canonicality alone is insufficient

Packed pages are essentially full and always capacity-safe, but a one-key update rewrites roughly half of the canonical leaf groups at this scale.

A fresh canonical full rewrite therefore has the identity property EXP-0003 wants, but not the physical locality needed for practical persistent updates.

### Bernoulli anchors prove expected locality alone is insufficient

Hash anchors have excellent update locality in this random ensemble, but the group-size tail is structurally incompatible with fixed pages:

- about 37% of groups exceed the 254-entry capacity;
- the mean per-seed maximum group is about 1,116 entries, more than four pages.

Adding a hard maximum by cutting oversized groups at a fixed rank would reintroduce history/cascade questions unless the repair rule itself is history-independent.

### Minimizers provide a surprisingly strong baseline

Window minimizers achieve all of the following in this model:

- current-set determinism;
- approximately one changed group for a one-key random edit;
- no group over the 254-entry hard bound;
- more than 99% of old keys remain inside exactly reused groups after a one-key edit.

The cost is occupancy distribution:

- mean group size is about 126.5, essentially half capacity;
- about 49% of groups are below the current 127-entry half-full floor;
- one-entry groups can occur.

So a simple minimizer scheme is **not** a drop-in EXP-0003 leaf policy. It is a constructive lower-complexity baseline showing that history independence + hard maximum + edit locality are simultaneously achievable, but not yet with the current minimum-occupancy guarantee.

## Grinding results

For ordinary midpoint insertion in the full ensemble, the inserted identifier becomes:

```text
hash-anchor boundary      ~0.0% observed (expected ~1/254 per random candidate)
minimizer boundary        ~0.4% observed
```

With 256-candidate priority grinding:

```text
hash-anchor boundary      ~61.0%
minimizer boundary        ~73.8%
```

The exact finite-sample rates are not normative. Their scale is the result: a writer-controlled identifier namespace can heavily steer any public deterministic hash-priority partition.

The ground insertion still changes only about 1.61 hash-anchor groups and 1.73 minimizer groups on average, so locality itself survives a **single** ground edit. Repeated adversarial construction is the important next stress case: it can drive boundary density and occupancy into regimes absent from the random model.

## Persistent round-trip result

For every seed, the current-style persistent leaf control produces:

```text
base groups                  99
final groups                 100
old groups reused            98 / 99
old keys in reused groups    98.984%
logical set restored         yes
leaf partition restored      no
```

This is the cleanest quantitative statement so far of the scoped-determinism trade-off:

> persistent split history changes only a small physical neighborhood, but that local reuse is exactly what makes the final representation depend on operation history.

## Relation to history-independent dynamic partitioning research

Bender, Farach-Colton, Goodrich, and Komlós recently solved the more general history-independent dynamic-partitioning problem. Their construction maintains ordered groups of size `Theta(B)` and processes insert/delete operations in `O(1)` expected operations against an oblivious adversary, with a high-probability bound, and they use it to construct a history-independent B-tree:

- *History-Independent Dynamic Partitioning: Operation-Order Privacy in Ordered Data Structures*, PACMMOD/PODS 2024, DOI `10.1145/3651609`;
- *History-Independent Dynamic Partitioning with Applications to B-Trees, Skip Lists and Fusion Trees*, ACM TODS 2026, DOI `10.1145/3810240`.

Experiment 0111 does **not** attempt to reproduce that algorithm from an abstract or adapt its proof by analogy. Instead, it establishes the UCOF-specific scorecard that any adaptation would have to beat:

```text
strict maximum page size
useful minimum occupancy
current-set / history-independent group identity
bounded or probabilistically controlled edit locality
small immutable write amplification
cross-language deterministic bytes
chosen-identifier adversarial robustness
authenticated page identity
```

The next research step can now evaluate the actual dynamic-partitioning construction against this scorecard rather than treating history independence as an all-or-nothing aesthetic goal.

## Relation to bounded-locality content-defined chunking

Berger's 2025 Chonkers work is another relevant construction family. It explicitly aims to provide strict chunk-size and edit-locality guarantees using deterministic hierarchical priority-based merging:

- Benjamin Berger, *The Chonkers Algorithm: Content-Defined Chunking with Provable Strict Guarantees on Size and Locality*, arXiv:2509.11121v2 (2025).

Its input/chunk semantics and periodic-data exceptions differ substantially from an ordered authenticated object directory, so direct adoption would be inappropriate. The important research clue is that **hierarchical deterministic merging can repair the size/locality conflict that defeats naïve anchors**.

That suggests a UCOF-specific follow-up: use ObjectId-derived local priorities only as a proto-partition, then apply a bounded deterministic merging layer whose proof obligations are stated directly in entry counts and page bytes.

## Decision impact

Experiment 0111 weakens the case for freezing history-sensitive persistent roots as the only plausible efficient design.

It does **not** establish a replacement. Instead it narrows the target:

1. packed canonical grouping is too non-local;
2. independent hash anchors have unacceptable size tails and chosen-ID steering;
3. simple minimizers provide strict maximum/locality but insufficient minimum occupancy;
4. current persistent repair provides excellent reuse but demonstrable history-sensitive identity;
5. a stronger history-independent dynamic partition / deterministic merging construction is now justified as a focused experiment before FCP-0003 accepts scoped determinism.

## Reproduction

Full evidence ensemble:

```console
python3 tools/experiment_exp0003_history_independent_partition.py
```

Short deterministic CI ensemble:

```console
python3 tools/experiment_exp0003_history_independent_partition.py --quick
```

Machine-readable output:

```console
python3 tools/experiment_exp0003_history_independent_partition.py --json
```
