use std::collections::BTreeMap;
use ucof_experiments::immutable_successor::{
    append_persistent_delete_experimental, append_persistent_insert, build_genesis,
    inspect_persistent_delete_leaf_frontier_experimental, rewrite_all,
    ExperimentalDeleteBorrowPolicy, ExperimentalDeleteLeafFrontier, ImmutableLimits,
    ImmutableObjectInput, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY,
};

const CYCLES_PER_TRACE: usize = 48;

#[derive(Clone, Copy)]
struct TraceSpec {
    name: &'static str,
    first_id: u64,
    pool_start: u64,
    pool_end: u64,
    seed: u64,
}

const TRACES: [TraceSpec; 5] = [
    TraceSpec {
        name: "whole-set-lcg",
        first_id: 186,
        pool_start: 92,
        pool_end: 379,
        seed: 0x9e37_79b9_7f4a_7c15,
    },
    TraceSpec {
        name: "left-leaf-hot",
        first_id: 92,
        pool_start: 92,
        pool_end: 185,
        seed: 0x243f_6a88_85a3_08d3,
    },
    TraceSpec {
        name: "middle-leaf-hot",
        first_id: 186,
        pool_start: 186,
        pool_end: 278,
        seed: 0x1319_8a2e_0370_7344,
    },
    TraceSpec {
        name: "right-leaf-hot",
        first_id: 279,
        pool_start: 279,
        pool_end: 379,
        seed: 0xa409_3822_299f_31d0,
    },
    TraceSpec {
        name: "left-middle-boundary-hot",
        first_id: 186,
        pool_start: 176,
        pool_end: 195,
        seed: 0x082e_fa98_ec4e_6c89,
    },
];

#[derive(Default)]
struct FrontierMetrics {
    observations: usize,
    underflows: usize,
    borrows: usize,
    merges: usize,
    donor_cliffs: usize,
    avoidable_donor_cliffs: usize,
    minimum_leaf_count_sum: usize,
    leaf_count_sum: usize,
}

impl FrontierMetrics {
    fn record(&mut self, frontier: &ExperimentalDeleteLeafFrontier) {
        self.observations += 1;
        self.minimum_leaf_count_sum += frontier.minimum_leaf_count;
        self.leaf_count_sum += frontier.leaf_count;
        if frontier.would_underflow {
            self.underflows += 1;
            if frontier.selected_donor_occupancy.is_some() {
                self.borrows += 1;
            } else {
                assert!(frontier.would_merge);
                self.merges += 1;
            }
        } else {
            assert!(frontier.selected_donor_occupancy.is_none());
            assert!(!frontier.would_merge);
        }
        if frontier.donor_cliff {
            self.donor_cliffs += 1;
        }
        if frontier.strictly_fuller_eligible_alternative {
            assert!(frontier.donor_cliff);
            self.avoidable_donor_cliffs += 1;
        }
    }

    fn assert_complete(&self) {
        assert_eq!(self.observations, CYCLES_PER_TRACE);
        assert_eq!(self.underflows, self.borrows + self.merges);
        assert!(self.donor_cliffs <= self.borrows);
        assert!(self.avoidable_donor_cliffs <= self.donor_cliffs);
    }
}

#[derive(Default)]
struct Metrics {
    delete_pages_written: usize,
    insert_pages_written: usize,
    delete_pages_reused: usize,
    insert_pages_reused: usize,
    bytes_appended: usize,
    delete_write_histogram: BTreeMap<usize, usize>,
    insert_write_histogram: BTreeMap<usize, usize>,
    frontier: FrontierMetrics,
}

impl Metrics {
    fn total_pages_written(&self) -> usize {
        self.delete_pages_written + self.insert_pages_written
    }

    fn total_pages_reused(&self) -> usize {
        self.delete_pages_reused + self.insert_pages_reused
    }

    fn record_delete(
        &mut self,
        frontier: &ExperimentalDeleteLeafFrontier,
        pages_written: usize,
        pages_reused: usize,
        appended_bytes: usize,
    ) {
        // These workloads remain at a depth-1 root. A non-root leaf borrow rewrites
        // target + donor + root (three pages); no-underflow and leaf merge cases
        // emit two pages. Keep this causal link executable rather than inferring it
        // later from aggregate histograms.
        assert_eq!(frontier.root_level, 1);
        assert_eq!(
            pages_written,
            if frontier.selected_donor_occupancy.is_some() {
                3
            } else {
                2
            }
        );
        self.frontier.record(frontier);
        self.delete_pages_written += pages_written;
        self.delete_pages_reused += pages_reused;
        self.bytes_appended += appended_bytes;
        *self
            .delete_write_histogram
            .entry(pages_written)
            .or_default() += 1;
    }

    fn record_insert(&mut self, pages_written: usize, pages_reused: usize, appended_bytes: usize) {
        self.insert_pages_written += pages_written;
        self.insert_pages_reused += pages_reused;
        self.bytes_appended += appended_bytes;
        *self
            .insert_write_histogram
            .entry(pages_written)
            .or_default() += 1;
    }

    fn histogram_total(histogram: &BTreeMap<usize, usize>) -> usize {
        histogram.values().sum()
    }

    fn histogram_weighted_pages(histogram: &BTreeMap<usize, usize>) -> usize {
        histogram.iter().map(|(pages, count)| pages * count).sum()
    }

    fn assert_histograms(&self) {
        assert_eq!(
            Self::histogram_total(&self.delete_write_histogram),
            CYCLES_PER_TRACE
        );
        assert_eq!(
            Self::histogram_total(&self.insert_write_histogram),
            CYCLES_PER_TRACE
        );
        assert_eq!(
            Self::histogram_weighted_pages(&self.delete_write_histogram),
            self.delete_pages_written
        );
        assert_eq!(
            Self::histogram_weighted_pages(&self.insert_write_histogram),
            self.insert_pages_written
        );
        assert_eq!(
            self.frontier.borrows,
            *self.delete_write_histogram.get(&3).unwrap_or(&0)
        );
        self.frontier.assert_complete();
    }
}

struct TraceResult {
    left: Metrics,
    fuller: Metrics,
    left_final_bytes: usize,
    fuller_final_bytes: usize,
    delete_divergent_cycles: usize,
    cycle_divergent_cycles: usize,
    left_smaller_after_cycle: usize,
    fuller_smaller_after_cycle: usize,
    equal_size_after_cycle: usize,
}

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, 1, vec![object_id as u8])
}

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(object)
        .collect()
}

fn comparison_fixture(limits: ImmutableLimits) -> Vec<u8> {
    assert_eq!(LEAF_CAPACITY, 185);
    assert_eq!(LEAF_MIN_OCCUPANCY, 93);

    let mut state = build_genesis(&objects(2 * LEAF_CAPACITY), limits).expect("two full leaves");
    for object_id in u64::try_from(2 * LEAF_CAPACITY + 1).expect("first insertion")..=379 {
        state = append_persistent_insert(&state, &object(object_id), limits)
            .expect("grow right sibling")
            .bytes;
    }

    let left_deletions = LEAF_CAPACITY - (LEAF_MIN_OCCUPANCY + 1);
    assert_eq!(left_deletions, 91);
    for object_id in 1..=u64::try_from(left_deletions).expect("left deletions") {
        state = append_persistent_delete_experimental(
            &state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        )
        .expect("shrink left sibling")
        .bytes;
    }
    state
}

fn next_object_id(spec: TraceSpec, cycle: usize, generator: &mut u64) -> u64 {
    assert!(spec.pool_start <= spec.first_id && spec.first_id <= spec.pool_end);
    if cycle == 0 {
        return spec.first_id;
    }

    *generator = generator
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    let pool_len = spec.pool_end - spec.pool_start + 1;
    spec.pool_start + (*generator % pool_len)
}

fn run_trace(
    base: &[u8],
    active_ids: &[u64],
    spec: TraceSpec,
    limits: ImmutableLimits,
) -> TraceResult {
    let mut left_state = base.to_vec();
    let mut fuller_state = base.to_vec();
    let mut left_metrics = Metrics::default();
    let mut fuller_metrics = Metrics::default();
    let mut delete_divergent_cycles = 0_usize;
    let mut cycle_divergent_cycles = 0_usize;
    let mut left_smaller_after_cycle = 0_usize;
    let mut fuller_smaller_after_cycle = 0_usize;
    let mut equal_size_after_cycle = 0_usize;
    let mut generator = spec.seed;

    for cycle in 0..CYCLES_PER_TRACE {
        let object_id = next_object_id(spec, cycle, &mut generator);

        let left_frontier = inspect_persistent_delete_leaf_frontier_experimental(
            &left_state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        )
        .expect("inspect left-first frontier");
        let left_before = left_state.len();
        let left_delete = append_persistent_delete_experimental(
            &left_state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        )
        .expect("left-first delete");
        left_metrics.record_delete(
            &left_frontier,
            left_delete.pages_written,
            left_delete.pages_reused,
            left_delete.bytes.len() - left_before,
        );

        let fuller_frontier = inspect_persistent_delete_leaf_frontier_experimental(
            &fuller_state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
        )
        .expect("inspect fuller-sibling frontier");
        let fuller_before = fuller_state.len();
        let fuller_delete = append_persistent_delete_experimental(
            &fuller_state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
        )
        .expect("fuller-sibling delete");
        fuller_metrics.record_delete(
            &fuller_frontier,
            fuller_delete.pages_written,
            fuller_delete.pages_reused,
            fuller_delete.bytes.len() - fuller_before,
        );

        assert_eq!(
            left_delete.report.object_count,
            fuller_delete.report.object_count
        );
        if left_delete.bytes != fuller_delete.bytes {
            delete_divergent_cycles += 1;
        }

        let left_before_insert = left_delete.bytes.len();
        let left_insert = append_persistent_insert(&left_delete.bytes, &object(object_id), limits)
            .expect("left-first reinsert");
        left_metrics.record_insert(
            left_insert.pages_written,
            left_insert.pages_reused,
            left_insert.bytes.len() - left_before_insert,
        );

        let fuller_before_insert = fuller_delete.bytes.len();
        let fuller_insert =
            append_persistent_insert(&fuller_delete.bytes, &object(object_id), limits)
                .expect("fuller-sibling reinsert");
        fuller_metrics.record_insert(
            fuller_insert.pages_written,
            fuller_insert.pages_reused,
            fuller_insert.bytes.len() - fuller_before_insert,
        );

        assert_eq!(left_insert.report.object_count, active_ids.len());
        assert_eq!(fuller_insert.report.object_count, active_ids.len());

        left_state = left_insert.bytes;
        fuller_state = fuller_insert.bytes;
        if left_state != fuller_state {
            cycle_divergent_cycles += 1;
        }
        match left_state.len().cmp(&fuller_state.len()) {
            std::cmp::Ordering::Less => left_smaller_after_cycle += 1,
            std::cmp::Ordering::Greater => fuller_smaller_after_cycle += 1,
            std::cmp::Ordering::Equal => equal_size_after_cycle += 1,
        }
    }

    left_metrics.assert_histograms();
    fuller_metrics.assert_histograms();
    assert_eq!(fuller_metrics.frontier.avoidable_donor_cliffs, 0);

    let left_fresh = rewrite_all(&left_state, limits).expect("canonicalize left-first trace");
    let fuller_fresh = rewrite_all(&fuller_state, limits).expect("canonicalize fuller trace");
    assert_eq!(left_fresh.retained_object_ids, active_ids);
    assert_eq!(fuller_fresh.retained_object_ids, active_ids);
    assert_eq!(left_fresh.bytes, fuller_fresh.bytes);

    TraceResult {
        left: left_metrics,
        fuller: fuller_metrics,
        left_final_bytes: left_state.len(),
        fuller_final_bytes: fuller_state.len(),
        delete_divergent_cycles,
        cycle_divergent_cycles,
        left_smaller_after_cycle,
        fuller_smaller_after_cycle,
        equal_size_after_cycle,
    }
}

fn print_policy(trace: &str, policy: &str, metrics: &Metrics, final_file_bytes: usize) {
    println!(
        "{trace},{policy},{CYCLES_PER_TRACE},{},{},{},{},{},{},{},{}",
        metrics.delete_pages_written,
        metrics.insert_pages_written,
        metrics.total_pages_written(),
        metrics.delete_pages_reused,
        metrics.insert_pages_reused,
        metrics.total_pages_reused(),
        metrics.bytes_appended,
        final_file_bytes,
    );
}

fn print_histogram(trace: &str, policy: &str, operation: &str, histogram: &BTreeMap<usize, usize>) {
    for (pages_written, count) in histogram {
        println!("histogram,{trace},{policy},{operation},{pages_written},{count}");
    }
}

fn print_frontier(trace: &str, policy: &str, frontier: &FrontierMetrics) {
    println!(
        "frontier,{trace},{policy},{},{},{},{},{},{},{},{}",
        frontier.observations,
        frontier.underflows,
        frontier.borrows,
        frontier.merges,
        frontier.donor_cliffs,
        frontier.avoidable_donor_cliffs,
        frontier.minimum_leaf_count_sum,
        frontier.leaf_count_sum,
    );
}

fn main() {
    let limits = ImmutableLimits::default();
    let base = comparison_fixture(limits);
    let active_ids: Vec<u64> = (92..=379).collect();
    assert_eq!(active_ids.len(), 288);

    println!("trace,policy,cycles,delete_pages_written,insert_pages_written,total_pages_written,delete_pages_reused,insert_pages_reused,total_pages_reused,bytes_appended,final_file_bytes");
    for spec in TRACES {
        let result = run_trace(&base, &active_ids, spec, limits);
        print_policy(
            spec.name,
            "left-first",
            &result.left,
            result.left_final_bytes,
        );
        print_policy(
            spec.name,
            "fuller-sibling",
            &result.fuller,
            result.fuller_final_bytes,
        );
        print_histogram(
            spec.name,
            "left-first",
            "delete",
            &result.left.delete_write_histogram,
        );
        print_histogram(
            spec.name,
            "left-first",
            "insert",
            &result.left.insert_write_histogram,
        );
        print_histogram(
            spec.name,
            "fuller-sibling",
            "delete",
            &result.fuller.delete_write_histogram,
        );
        print_histogram(
            spec.name,
            "fuller-sibling",
            "insert",
            &result.fuller.insert_write_histogram,
        );
        print_frontier(spec.name, "left-first", &result.left.frontier);
        print_frontier(spec.name, "fuller-sibling", &result.fuller.frontier);
        println!(
            "# {name}: delete_divergent_cycles={delete},cycle_divergent_cycles={cycle},left_smaller={left_smaller},fuller_smaller={fuller_smaller},equal_size={equal}",
            name = spec.name,
            delete = result.delete_divergent_cycles,
            cycle = result.cycle_divergent_cycles,
            left_smaller = result.left_smaller_after_cycle,
            fuller_smaller = result.fuller_smaller_after_cycle,
            equal = result.equal_size_after_cycle,
        );
    }
    println!("canonical_fresh_bytes_equal_for_all_traces=1");
}
