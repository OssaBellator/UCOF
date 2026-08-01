use std::fmt;
use std::io::Write;

use crate::immutable_successor::{
    rewrite_versioned_source_sequence_selected_to, ImmutableHistoryToSinkError,
    ImmutableHistoryToSinkReport, ImmutableSourceLimits, ImmutableSourceStreamingWriteOptions,
    ImmutableVersionedReadAt,
};
use crate::{CompactionError, CompactionLimits, CompactionPlan, ObjectGraph};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableHistoricalSemanticStreamingError {
    Compaction(CompactionError),
    Historical(ImmutableHistoryToSinkError),
}

impl fmt::Display for ImmutableHistoricalSemanticStreamingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compaction(error) => {
                write!(formatter, "historical semantic compaction failed: {error}")
            }
            Self::Historical(error) => {
                write!(formatter, "historical semantic streaming failed: {error}")
            }
        }
    }
}

impl std::error::Error for ImmutableHistoricalSemanticStreamingError {}

impl From<CompactionError> for ImmutableHistoricalSemanticStreamingError {
    fn from(error: CompactionError) -> Self {
        Self::Compaction(error)
    }
}

impl From<ImmutableHistoryToSinkError> for ImmutableHistoricalSemanticStreamingError {
    fn from(error: ImmutableHistoryToSinkError) -> Self {
        Self::Historical(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableHistoricalSemanticStreamingReport {
    pub plan: CompactionPlan,
    pub output: ImmutableHistoryToSinkReport,
}

/// Computes bounded dependency closure, selects one exact linked-history sequence from a strongly
/// versioned bounded source, and streams exactly the reachable active objects into a new canonical
/// genesis file.
///
/// Graph planning completes before source validation or sink output. The caller supplies the graph;
/// opaque payload bytes are not interpreted here. Complete linked-history and selected-prefix
/// validation occur under one non-ABA source version and one cumulative source budget. Only reachable
/// payloads are reread for emission, although strict selected-prefix inventory may read all active
/// payloads once. Historical identity, extensions, provenance, and signatures are not preserved.
pub fn rewrite_compacted_versioned_history_sequence_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    sequence: u64,
    graph: &ObjectGraph,
    selected_roots: &[u64],
    compaction_limits: CompactionLimits,
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<ImmutableHistoricalSemanticStreamingReport, ImmutableHistoricalSemanticStreamingError> {
    let plan = graph.plan(selected_roots, compaction_limits)?;
    let output = rewrite_versioned_source_sequence_selected_to(
        writer,
        source,
        sequence,
        &plan.reachable,
        source_limits,
        options,
    )?;
    Ok(ImmutableHistoricalSemanticStreamingReport { plan, output })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immutable_successor::{
        append_replacement, build_genesis, rewrite_selected, validate_history, ImmutableLimits,
        ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError,
        ImmutableStreamingWriteOptions,
    };

    #[derive(Clone, Debug)]
    struct VersionedMemorySource {
        data: Vec<u8>,
        version: [u8; 32],
        largest_request: usize,
    }

    impl ImmutableReadAt for VersionedMemorySource {
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
            self.largest_request = self.largest_request.max(buffer.len());
            Ok(())
        }
    }

    impl ImmutableVersionedReadAt for VersionedMemorySource {
        fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError> {
            Ok(self.version)
        }
    }

    fn object(object_id: u64, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 31).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("seed"); payload_len],
        )
    }

    fn graph() -> ObjectGraph {
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![2, 3]).expect("object 1");
        graph.add_object(2, vec![4]).expect("object 2");
        graph.add_object(3, Vec::new()).expect("object 3");
        graph.add_object(4, Vec::new()).expect("object 4");
        graph.add_object(5, Vec::new()).expect("orphan 5");
        graph
    }

    fn historical_source(format: ImmutableLimits) -> Vec<u8> {
        let genesis = build_genesis(
            &[
                object(1, 11),
                object(2, 13),
                object(3, 17),
                object(4, 19),
                object(5, 4_096),
            ],
            format,
        )
        .expect("genesis");
        append_replacement(
            &genesis,
            &ImmutableObjectInput::new(1, 77, b"active-one".to_vec()),
            format,
        )
        .expect("replacement")
    }

    #[test]
    fn historical_dependency_closure_matches_owned_prefix_selection() {
        let format = ImmutableLimits::default();
        let data = historical_source(format);
        let history = validate_history(&data, format).expect("history");
        let entry = history
            .entries
            .iter()
            .find(|entry| entry.report.sequence == 0)
            .expect("sequence zero");
        let prefix_len = entry.footer_offset + 192;
        let expected = rewrite_selected(
            &data[..usize::try_from(prefix_len).expect("prefix")],
            &[1, 2, 3, 4],
            format,
        )
        .expect("owned selection");

        let mut source = VersionedMemorySource {
            data,
            version: [61; 32],
            largest_request: 0,
        };
        let mut actual = Vec::new();
        let report = rewrite_compacted_versioned_history_sequence_to(
            &mut actual,
            &mut source,
            0,
            &graph(),
            &[1],
            CompactionLimits::default(),
            ImmutableSourceLimits {
                format,
                max_total_bytes_read: 64 * 1024 * 1024,
                max_read_operations: 1_000_000,
                max_read_request_bytes: 31,
                hash_block_bytes: 29,
            },
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 37,
                },
                max_source_read_bytes: 17,
            },
        )
        .expect("historical semantic streaming");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.plan.reachable, vec![1, 2, 3, 4]);
        assert_eq!(report.plan.orphaned, vec![5]);
        assert_eq!(report.plan.edges_visited, 3);
        assert_eq!(report.plan.maximum_depth, 2);
        assert_eq!(
            report.output.output.cumulative_source_stats.bytes_read
                - report.output.output.inventory_stats.bytes_read,
            11 + 13 + 17 + 19
        );
        assert!(report.output.output.largest_payload_read_request <= 17);
        assert!(source.largest_request <= 31);
    }

    #[test]
    fn graph_failure_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![9]).expect("object 1");
        let mut source = VersionedMemorySource {
            data: historical_source(format),
            version: [67; 32],
            largest_request: 0,
        };
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_compacted_versioned_history_sequence_to(
                &mut sink,
                &mut source,
                0,
                &graph,
                &[1],
                CompactionLimits::default(),
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            ),
            Err(ImmutableHistoricalSemanticStreamingError::Compaction(
                CompactionError::MissingObject(9)
            ))
        );
        assert!(sink.is_empty());
    }

    #[test]
    fn reachable_object_missing_from_history_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let mut graph = graph();
        graph.add_object(9, Vec::new()).expect("object 9");
        graph.add_object(8, vec![9]).expect("object 8");
        let mut source = VersionedMemorySource {
            data: historical_source(format),
            version: [71; 32],
            largest_request: 0,
        };
        let mut sink = Vec::new();
        assert!(matches!(
            rewrite_compacted_versioned_history_sequence_to(
                &mut sink,
                &mut source,
                0,
                &graph,
                &[8],
                CompactionLimits::default(),
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            ),
            Err(ImmutableHistoricalSemanticStreamingError::Historical(_))
        ));
        assert!(sink.is_empty());
    }
}
