#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{
    immutable_successor::{
        append_replacement, build_genesis, rewrite_selected, ImmutableLimits, ImmutableObjectInput,
        ImmutableSourceStreamingWriteOptions, ImmutableStreamingWriteOptions,
    },
    rewrite_compacted_active_file_to, CompactionLimits, ObjectGraph,
};

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; payload_len])
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte % 15));
    let source_chunk = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let sink_chunk = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let limits = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let mut active_lengths = Vec::with_capacity(count);
    let objects: Vec<_> = (0..count)
        .map(|index| {
            let object_id = u64::try_from(index + 1).expect("small object id");
            let seed = data.get(index + 3).copied().unwrap_or(index as u8);
            let payload_len = 1 + usize::from(seed % 96);
            active_lengths.push(payload_len);
            object(object_id, seed, payload_len)
        })
        .collect();
    let genesis = build_genesis(&objects, limits).expect("bounded genesis");

    let replacement_index = data
        .get(3 + count)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let replacement_seed = data.get(4 + count).copied().unwrap_or(101);
    let replacement_len = 1 + usize::from(replacement_seed % 96);
    let replacement_id = u64::try_from(replacement_index + 1).expect("small object id");
    let source = append_replacement(
        &genesis,
        &object(replacement_id, replacement_seed, replacement_len),
        limits,
    )
    .expect("bounded replacement");
    active_lengths[replacement_index] = replacement_len;

    let mut graph = ObjectGraph::new();
    for index in 0..count {
        let object_id = u64::try_from(index + 1).expect("small object id");
        let selector = data.get(5 + count + index).copied().unwrap_or(index as u8);
        let mut dependencies = Vec::new();
        if index > 0 && selector & 1 != 0 {
            dependencies.push(u64::try_from(index).expect("previous object id"));
        }
        if index > 1 && selector & 2 != 0 {
            dependencies.push(u64::try_from(index - 1).expect("second previous object id"));
        }
        if index + 1 == count && selector & 4 != 0 {
            dependencies.push(1);
        }
        dependencies.sort_unstable();
        dependencies.dedup();
        graph.add_object(object_id, dependencies).expect("unique graph object");
    }

    let mut roots = vec![u64::try_from(count).expect("root")];
    if data.get(5 + 2 * count).is_some_and(|byte| byte & 1 != 0) {
        roots.push(1);
        roots.reverse();
    }
    let compaction_limits = CompactionLimits {
        max_nodes: 32,
        max_edges: 64,
        max_depth: 32,
    };
    let plan = graph
        .plan(&roots, compaction_limits)
        .expect("bounded graph plan");
    let expected = rewrite_selected(&source, &plan.reachable, limits).expect("owned selection");
    let expected_read_bytes: u64 = plan
        .reachable
        .iter()
        .map(|object_id| {
            let index = usize::try_from(*object_id - 1).expect("small index");
            u64::try_from(active_lengths[index]).expect("bounded payload")
        })
        .sum();

    let mut actual = Vec::new();
    let report = rewrite_compacted_active_file_to(
        &mut actual,
        &source,
        &graph,
        &roots,
        compaction_limits,
        ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: sink_chunk,
            },
            max_source_read_bytes: source_chunk,
        },
        limits,
    )
    .expect("semantic streaming");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.plan, plan);
    assert_eq!(report.output.output.source_bytes_read, expected_read_bytes);
    assert!(report.output.largest_payload_read_request <= source_chunk);
    assert!(report.output.output.output.largest_write_request <= sink_chunk);

    let mut invalid_graph = ObjectGraph::new();
    invalid_graph
        .add_object(1, vec![u64::try_from(count + 1).expect("missing id")])
        .expect("invalid root");
    let mut untouched = Vec::new();
    assert!(rewrite_compacted_active_file_to(
        &mut untouched,
        &source,
        &invalid_graph,
        &[1],
        compaction_limits,
        ImmutableSourceStreamingWriteOptions::default(),
        limits,
    )
    .is_err());
    assert!(untouched.is_empty());
});
