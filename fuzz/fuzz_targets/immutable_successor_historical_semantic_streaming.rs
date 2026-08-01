#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, rewrite_selected, validate_history, ImmutableLimits,
    ImmutableObjectInput, ImmutableReadAt, ImmutableSelectedHistoryToSinkError,
    ImmutableSourceError, ImmutableSourceLimits, ImmutableSourceStreamingWriteOptions,
    ImmutableStreamingWriteOptions, ImmutableVersionedReadAt, FOOTER_LEN,
};
use ucof_experiments::{
    rewrite_compacted_versioned_source_sequence_to, CompactionError, CompactionLimits,
    ImmutableHistoricalSemanticStreamingError, ImmutableHistoricalSemanticStreamingOptions,
    ObjectGraph,
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

fn object(object_id: u64, seed: u8) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::from(1 + seed % 31),
        vec![seed; 1 + usize::from(seed % 96)],
    )
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 8));
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
        max_objects: 16,
        max_pages: 64,
        max_depth: 4,
        max_history_entries: 8,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let mut state: Vec<_> = (0..count)
        .map(|index| {
            let seed = data.get(index + 6).copied().unwrap_or(index as u8);
            object(u64::try_from(index + 1).expect("small object id"), seed)
        })
        .collect();
    let mut states = vec![state.clone()];
    let mut source_bytes = build_genesis(&state, format).expect("bounded genesis");
    for commit in 1..commit_count {
        let index = data
            .get(6 + count + commit)
            .map_or(commit % count, |byte| usize::from(*byte) % count);
        let seed = data
            .get(6 + count + commit_count + commit)
            .copied()
            .unwrap_or(71 + commit as u8);
        let replacement = object(
            u64::try_from(index + 1).expect("small object id"),
            seed,
        );
        source_bytes = append_replacement(&source_bytes, &replacement, format)
            .expect("bounded replacement");
        state[index] = replacement;
        states.push(state.clone());
    }

    let sequence = data.last().map_or(0_usize, |byte| {
        usize::from(*byte) % commit_count
    });
    let root_index = data
        .get(6 + count + commit_count * 2)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let mut graph = ObjectGraph::new();
    for index in 0..count {
        let dependency = if index + 1 < count
            && data
                .get(7 + count + commit_count * 2 + index)
                .is_some_and(|byte| byte & 1 != 0)
        {
            vec![u64::try_from(index + 2).expect("dependency id")]
        } else {
            Vec::new()
        };
        graph
            .add_object(
                u64::try_from(index + 1).expect("graph object id"),
                dependency,
            )
            .expect("acyclic graph object");
    }
    let root = u64::try_from(root_index + 1).expect("root id");
    let plan = graph
        .plan(&[root], CompactionLimits::default())
        .expect("bounded graph plan");

    let history = validate_history(&source_bytes, format).expect("slice history");
    let entry = history
        .entries
        .iter()
        .find(|entry| entry.report.sequence == u64::try_from(sequence).expect("sequence"))
        .expect("selected sequence");
    let prefix_len = entry.footer_offset + u64::try_from(FOOTER_LEN).expect("footer length");
    let prefix = &source_bytes[..usize::try_from(prefix_len).expect("prefix")];
    let expected = rewrite_selected(prefix, &plan.reachable, format).expect("owned selection");
    let expected_payload_bytes: u64 = plan
        .reachable
        .iter()
        .map(|object_id| {
            let index = usize::try_from(*object_id - 1).expect("state index");
            u64::try_from(states[sequence][index].payload.len()).expect("payload length")
        })
        .sum();
    let options = ImmutableHistoricalSemanticStreamingOptions {
        compaction: CompactionLimits::default(),
        source: ImmutableSourceLimits {
            format,
            max_total_bytes_read: 32 * 1024 * 1024,
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
        version: [79; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut actual = Vec::new();
    let report = rewrite_compacted_versioned_source_sequence_to(
        &mut actual,
        &mut source,
        &graph,
        &[root],
        u64::try_from(sequence).expect("sequence"),
        options,
    )
    .expect("historical semantic streaming");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.plan, plan);
    assert_eq!(report.output.output.selected_object_ids, plan.reachable);
    assert_eq!(report.output.output.output.output.report, expected.output);
    assert_eq!(
        report.output.output.output.cumulative_source_stats.bytes_read
            - report.output.output.output.inventory_stats.bytes_read,
        expected_payload_bytes
    );
    assert!(source.largest_request <= request);
    assert!(report.output.output.output.largest_payload_read_request <= payload_chunk.min(request));
    assert!(report.output.output.output.output.largest_write_request <= sink_chunk);

    let mut invalid_graph = ObjectGraph::new();
    invalid_graph
        .add_object(1, vec![u64::try_from(count + 1).expect("missing id")])
        .expect("invalid graph root");
    let mut untouched_source = VersionedSource {
        data: source_bytes.clone(),
        version: [83; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut untouched_sink = Vec::new();
    assert_eq!(
        rewrite_compacted_versioned_source_sequence_to(
            &mut untouched_sink,
            &mut untouched_source,
            &invalid_graph,
            &[1],
            u64::try_from(sequence).expect("sequence"),
            options,
        ),
        Err(ImmutableHistoricalSemanticStreamingError::Compaction(
            CompactionError::MissingObject(
                u64::try_from(count + 1).expect("missing object id")
            )
        ))
    );
    assert_eq!(untouched_source.reads, 0);
    assert!(untouched_sink.is_empty());

    let mut unstable = VersionedSource {
        data: source_bytes,
        version: [89; 32],
        reads: 0,
        mutate_after: Some(2),
        largest_request: 0,
    };
    assert_eq!(
        rewrite_compacted_versioned_source_sequence_to(
            &mut untouched_sink,
            &mut unstable,
            &graph,
            &[root],
            u64::try_from(sequence).expect("sequence"),
            options,
        ),
        Err(ImmutableHistoricalSemanticStreamingError::Streaming(
            ImmutableSelectedHistoryToSinkError::VersionChanged
        ))
    );
    assert!(untouched_sink.is_empty());
});
