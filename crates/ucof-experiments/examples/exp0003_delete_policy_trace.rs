use ucof_experiments::immutable_successor::{
    append_persistent_delete_experimental, append_persistent_insert, build_genesis, rewrite_all,
    ExperimentalDeleteBorrowPolicy, ImmutableLimits, ImmutableObjectInput, LEAF_CAPACITY,
    LEAF_MIN_OCCUPANCY,
};

const CYCLES: usize = 96;

#[derive(Default)]
struct Metrics {
    delete_pages_written: usize,
    insert_pages_written: usize,
    delete_pages_reused: usize,
    insert_pages_reused: usize,
    bytes_appended: usize,
}

impl Metrics {
    fn total_pages_written(&self) -> usize {
        self.delete_pages_written + self.insert_pages_written
    }

    fn total_pages_reused(&self) -> usize {
        self.delete_pages_reused + self.insert_pages_reused
    }
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

fn main() {
    let limits = ImmutableLimits::default();
    let base = comparison_fixture(limits);
    let active_ids: Vec<u64> = (92..=379).collect();
    assert_eq!(active_ids.len(), 288);

    let mut left_state = base.clone();
    let mut fuller_state = base;
    let mut left_metrics = Metrics::default();
    let mut fuller_metrics = Metrics::default();
    let mut delete_divergent_cycles = 0_usize;
    let mut cycle_divergent_cycles = 0_usize;
    let mut left_smaller_after_cycle = 0_usize;
    let mut fuller_smaller_after_cycle = 0_usize;
    let mut equal_size_after_cycle = 0_usize;
    let mut generator = 0x9e37_79b9_7f4a_7c15_u64;

    for cycle in 0..CYCLES {
        let object_id = if cycle == 0 {
            // The exact one-step fixture from Experiment 0112: middle leaf underflows
            // with both siblings eligible and the right sibling fuller.
            186
        } else {
            generator = generator
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            active_ids[usize::try_from(generator % active_ids.len() as u64).expect("index")]
        };

        let left_before = left_state.len();
        let left_delete = append_persistent_delete_experimental(
            &left_state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        )
        .expect("left-first delete");
        left_metrics.delete_pages_written += left_delete.pages_written;
        left_metrics.delete_pages_reused += left_delete.pages_reused;
        left_metrics.bytes_appended += left_delete.bytes.len() - left_before;

        let fuller_before = fuller_state.len();
        let fuller_delete = append_persistent_delete_experimental(
            &fuller_state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
        )
        .expect("fuller-sibling delete");
        fuller_metrics.delete_pages_written += fuller_delete.pages_written;
        fuller_metrics.delete_pages_reused += fuller_delete.pages_reused;
        fuller_metrics.bytes_appended += fuller_delete.bytes.len() - fuller_before;

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
        left_metrics.insert_pages_written += left_insert.pages_written;
        left_metrics.insert_pages_reused += left_insert.pages_reused;
        left_metrics.bytes_appended += left_insert.bytes.len() - left_before_insert;

        let fuller_before_insert = fuller_delete.bytes.len();
        let fuller_insert =
            append_persistent_insert(&fuller_delete.bytes, &object(object_id), limits)
                .expect("fuller-sibling reinsert");
        fuller_metrics.insert_pages_written += fuller_insert.pages_written;
        fuller_metrics.insert_pages_reused += fuller_insert.pages_reused;
        fuller_metrics.bytes_appended += fuller_insert.bytes.len() - fuller_before_insert;

        assert_eq!(left_insert.report.object_count, active_ids.len());
        assert_eq!(fuller_insert.report.object_count, active_ids.len());
        assert_eq!(
            left_insert.report.object_count,
            fuller_insert.report.object_count
        );

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

    let left_fresh = rewrite_all(&left_state, limits).expect("canonicalize left-first trace");
    let fuller_fresh = rewrite_all(&fuller_state, limits).expect("canonicalize fuller trace");
    assert_eq!(left_fresh.retained_object_ids, active_ids);
    assert_eq!(fuller_fresh.retained_object_ids, active_ids);
    assert_eq!(left_fresh.bytes, fuller_fresh.bytes);

    println!("policy,cycles,delete_pages_written,insert_pages_written,total_pages_written,delete_pages_reused,insert_pages_reused,total_pages_reused,bytes_appended,final_file_bytes");
    println!(
        "left-first,{CYCLES},{},{},{},{},{},{},{},{}",
        left_metrics.delete_pages_written,
        left_metrics.insert_pages_written,
        left_metrics.total_pages_written(),
        left_metrics.delete_pages_reused,
        left_metrics.insert_pages_reused,
        left_metrics.total_pages_reused(),
        left_metrics.bytes_appended,
        left_state.len(),
    );
    println!(
        "fuller-sibling,{CYCLES},{},{},{},{},{},{},{},{}",
        fuller_metrics.delete_pages_written,
        fuller_metrics.insert_pages_written,
        fuller_metrics.total_pages_written(),
        fuller_metrics.delete_pages_reused,
        fuller_metrics.insert_pages_reused,
        fuller_metrics.total_pages_reused(),
        fuller_metrics.bytes_appended,
        fuller_state.len(),
    );
    println!("delete_divergent_cycles={delete_divergent_cycles}");
    println!("cycle_divergent_cycles={cycle_divergent_cycles}");
    println!("left_smaller_after_cycle={left_smaller_after_cycle}");
    println!("fuller_smaller_after_cycle={fuller_smaller_after_cycle}");
    println!("equal_size_after_cycle={equal_size_after_cycle}");
    println!("canonical_fresh_bytes_equal=1");
}
