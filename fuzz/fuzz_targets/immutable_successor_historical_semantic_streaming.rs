#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{
    immutable_successor::{
        append_replacement, build_genesis, rewrite_selected, validate_history, ImmutableLimits,
        ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableSourceLimits,
        ImmutableSourceStreamingWriteOptions, ImmutableStreamingWriteOptions,
        ImmutableVersionedReadAt, FOOTER_LEN,
    },
    rewrite_compacted_versioned_history_sequence_to, CompactionLimits,
    ImmutableHistoricalSemanticStreamingOptions, ObjectGraph,
};

#[derive(Clone, Debug)]
struct VersionedSource {
    data: Vec<u8>,
    version: [u8; 32],
    reads: u64,
    mutate_after: Option<u64>,
    largest_request: usize,
}

impl ImmutableReadAt for VersionedSource {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.data.len()).map_err(|_| ImmutableSourceError::Limit("length"))
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(
            self.data
                .get(start..end)
                .ok_or(ImmutableSourceError::Io("range"))?,
        );
        self.reads += 1;
        self.largest_request = self.largest_request.max(buffer.len());
        if self.mutate_after == Some(self.reads) {
            self.version[0] ^= 1;
        }
        Ok(())
    }
}

impl ImmutableVersionedReadAt for VersionedSource {
    fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError> {
        Ok(self.version)
    }
}

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; payload_len])
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte % 11));
    let commit_count = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 3));
    let request = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let hash_block = data
        .get(3)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let payload_chunk = data
        .get(4)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let sink_chunk = data
        .get(5)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let format = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_history_entries: 8,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let mut payload_lengths = Vec::with_capacity(count);
    let objects: Vec<_> = (0..count)
        .map(|index| {
            let object_id = u64::try_from(index + 1).expect("small object id");
            let seed = data.get(index + 6).copied().unwrap_or(index as u8);
            let payload_len = 1 + usize::from(seed % 96);
            payload_lengths.push(payload_len);
            object(object_id, seed, payload_len)
        })
        .collect();
    let mut source_bytes = build_genesis(&objects, format).expect("bounded genesis");
    for commit in 1..commit_count {
        let index = data
            .get(6 + count + commit)
            .map_or(commit % count, |byte| usize::from(*byte) % count);
        let seed = data
            .get(6 + count + commit_count + commit)
            .copied()
            .unwrap_or(101 + commit as u8);
        source_bytes = append_replacement(
            &source_bytes,
            &object(
                u64::try_from(index + 1).expect("small object id"),
                seed,
                payload_lengths[index],
            ),
            format,
        )
        .expect("bounded replacement");
    }

    let mut graph = ObjectGraph::new();
    for index in 0..count {
        let object_id = u64::try_from(index + 1).expect("small object id");
        let selector = data
            .get(6 + count + 2 * commit_count + index)
            .copied()
            .unwrap_or(index as u8);
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
        graph
            .add_object(object_id, dependencies)
            .expect("unique graph object");
    }

    let mut roots = vec![u64::try_from(count).expect("root")];
    if data
        .get(6 + 2 * count + 2 * commit_count)
        .is_some_and(|byte| byte & 1 != 0)
    {
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

    let sequence = data.last().map_or(0_u64, |byte| {
        u64::from(*byte) % u64::try_from(commit_count).expect("commit count")
    });
    let history = validate_history(&source_bytes, format).expect("history");
    let entry = history
        .entries
        .iter()
        .find(|entry| entry.report.sequence == sequence)
        .expect("selected sequence");
    let prefix_len = entry.footer_offset + u64::try_from(FOOTER_LEN).expect("footer length");
    let expected = rewrite_selected(
        &source_bytes[..usize::try_from(prefix_len).expect("prefix")],
        &plan.reachable,
        format,
    )
    .expect("owned historical selection");
    let expected_payload_bytes: u64 = plan
        .reachable
        .iter()
        .map(|object_id| {
            let index = usize::try_from(*object_id - 1).expect("small object index");
            u64::try_from(payload_lengths[index]).expect("bounded payload")
        })
        .sum();
    let semantic_options = ImmutableHistoricalSemanticStreamingOptions {
        compaction: compaction_limits,
        source: ImmutableSourceLimits {
            format,
            max_total_bytes_read: 64 * 1024 * 1024,
            max_read_operations: 1_000_000,
            max_read_request_bytes: request,
            hash_block_bytes: hash_block,
        },
        output: ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: sink_chunk,
            },
            max_source_read_bytes: payload_chunk,
        },
    };

    let mut source = VersionedSource {
        data: source_bytes.clone(),
        version: [73; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut actual = Vec::new();
    let report = rewrite_compacted_versioned_history_sequence_to(
        &mut actual,
        &mut source,
        sequence,
        &graph,
        &roots,
        semantic_options,
    )
    .expect("historical semantic streaming");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.plan, plan);
    assert_eq!(report.output.selected_prefix_len, prefix_len);
    assert_eq!(report.output.output.output.report, expected.output);
    assert_eq!(
        report.output.output.cumulative_source_stats.bytes_read
            - report.output.output.inventory_stats.bytes_read,
        expected_payload_bytes
    );
    assert!(report.output.output.largest_payload_read_request <= payload_chunk.min(request));
    assert!(report.output.output.output.largest_write_request <= sink_chunk);
    assert!(source.largest_request <= request);

    let mut invalid_graph = ObjectGraph::new();
    invalid_graph
        .add_object(1, vec![u64::try_from(count + 1).expect("missing id")])
        .expect("invalid root");
    let mut untouched = Vec::new();
    assert!(rewrite_compacted_versioned_history_sequence_to(
        &mut untouched,
        &mut source,
        sequence,
        &invalid_graph,
        &[1],
        semantic_options,
    )
    .is_err());
    assert!(untouched.is_empty());

    let missing_id = u64::try_from(count + 1).expect("missing source id");
    let mut missing_graph = ObjectGraph::new();
    missing_graph
        .add_object(missing_id, Vec::new())
        .expect("missing source object");
    let mut missing_source = VersionedSource {
        data: source_bytes.clone(),
        version: [79; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    assert!(rewrite_compacted_versioned_history_sequence_to(
        &mut untouched,
        &mut missing_source,
        sequence,
        &missing_graph,
        &[missing_id],
        semantic_options,
    )
    .is_err());
    assert!(untouched.is_empty());

    let mut unstable = VersionedSource {
        data: source_bytes,
        version: [83; 32],
        reads: 0,
        mutate_after: Some(2),
        largest_request: 0,
    };
    assert!(rewrite_compacted_versioned_history_sequence_to(
        &mut untouched,
        &mut unstable,
        sequence,
        &graph,
        &roots,
        semantic_options,
    )
    .is_err());
    assert!(untouched.is_empty());
});
