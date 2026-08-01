use std::fmt;
use std::io::Write;

use crate::immutable_successor::{
    rewrite_selected_versioned_source_sequence_to, ImmutableSelectedHistoryToSinkError,
    ImmutableSelectedHistoryToSinkReport, ImmutableSourceLimits,
    ImmutableSourceStreamingWriteOptions, ImmutableVersionedReadAt,
};
use crate::{CompactionError, CompactionLimits, CompactionPlan, ObjectGraph};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableHistoricalSemanticStreamingOptions {
    pub compaction: CompactionLimits,
    pub source: ImmutableSourceLimits,
    pub output: ImmutableSourceStreamingWriteOptions,
}

impl Default for ImmutableHistoricalSemanticStreamingOptions {
    fn default() -> Self {
        Self {
            compaction: CompactionLimits::default(),
            source: ImmutableSourceLimits::default(),
            output: ImmutableSourceStreamingWriteOptions::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableHistoricalSemanticStreamingError {
    Compaction(CompactionError),
    Streaming(ImmutableSelectedHistoryToSinkError),
}

impl fmt::Display for ImmutableHistoricalSemanticStreamingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compaction(error) => write!(formatter, "historical compaction failed: {error}"),
            Self::Streaming(error) => {
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

impl From<ImmutableSelectedHistoryToSinkError>
    for ImmutableHistoricalSemanticStreamingError
{
    fn from(error: ImmutableSelectedHistoryToSinkError) -> Self {
        Self::Streaming(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableHistoricalSemanticStreamingReport {
    pub plan: CompactionPlan,
    pub output: ImmutableSelectedHistoryToSinkReport,
}

/// Computes bounded dependency closure before touching the source, selects one authenticated linked
/// historical sequence, and streams exactly the reachable objects into a new canonical genesis file.
///
/// The graph is caller-supplied and is not inferred from opaque payload bytes. Strict history and
/// selected-prefix validation still authenticate every active object in the chosen state. Only the
/// second payload pass and output are restricted to the reachable closure.
pub fn rewrite_compacted_versioned_source_sequence_to<
    W: Write,
    S: ImmutableVersionedReadAt,
>(
    writer: &mut W,
    source: &mut S,
    graph: &ObjectGraph,
    selected_roots: &[u64],
    sequence: u64,
    options: ImmutableHistoricalSemanticStreamingOptions,
) -> Result<ImmutableHistoricalSemanticStreamingReport, ImmutableHistoricalSemanticStreamingError> {
    let plan = graph.plan(selected_roots, options.compaction)?;
    let output = rewrite_selected_versioned_source_sequence_to(
        writer,
        source,
        sequence,
        &plan.reachable,
        options.source,
        options.output,
    )?;
    Ok(ImmutableHistoricalSemanticStreamingReport { plan, output })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immutable_successor::{
        append_replacement, build_genesis, rewrite_selected, validate_history, ImmutableLimits,
        ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableStreamingWriteOptions,
        FOOTER_LEN,
    };

    #[derive(Clone, Debug)]
    struct VersionedMemorySource {
        data: Vec<u8>,
        version: [u8; 32],
        reads: u64,
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
            self.reads += 1;
            self.largest_request = self.largest_request.max(buffer.len());
            Ok(())
        }
    }

    impl ImmutableVersionedReadAt for VersionedMemorySource {
        fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError> {
            Ok(self.version)
        }
    }

    fn object(object_id: u64, payload: &[u8]) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 23).expect("kind"),
            payload.to_vec(),
        )
    }

    fn source_file(format: ImmutableLimits) -> Vec<u8> {
        let genesis = build_genesis(
            &[
                object(1, b"root-at-zero"),
                object(2, b"dependency-two"),
                object(3, b"dependency-three"),
                object(4, b"dependency-four"),
                object(5, &[91; 4_096]),
            ],
            format,
        )
        .expect("genesis");
        append_replacement(
            &genesis,
            &ImmutableObjectInput::new(1, 77, b"root-at-one".to_vec()),
            format,
        )
        .expect("replacement")
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

    #[test]
    fn historical_dependency_closure_matches_owned_prefix_selection() {
        let format = ImmutableLimits::default();
        let data = source_file(format);
        let history = validate_history(&data, format).expect("history");
        let entry = history
            .entries
            .iter()
            .find(|entry| entry.report.sequence == 1)
            .expect("sequence one");
        let prefix_len = entry.footer_offset + FOOTER_LEN as u64;
        let prefix = &data[..usize::try_from(prefix_len).expect("prefix")];
        let expected = rewrite_selected(prefix, &[1, 2, 3, 4], format).expect("selection");

        let mut source = VersionedMemorySource {
            data,
            version: [67; 32],
            reads: 0,
            largest_request: 0,
        };
        let mut actual = Vec::new();
        let report = rewrite_compacted_versioned_source_sequence_to(
            &mut actual,
            &mut source,
            &graph(),
            &[1],
            1,
            ImmutableHistoricalSemanticStreamingOptions {
                compaction: CompactionLimits::default(),
                source: ImmutableSourceLimits {
                    format,
                    max_total_bytes_read: 64 * 1024 * 1024,
                    max_read_operations: 1_000_000,
                    max_read_request_bytes: 31,
                    hash_block_bytes: 29,
                },
                output: ImmutableSourceStreamingWriteOptions {
                    output: ImmutableStreamingWriteOptions {
                        max_write_request_bytes: 37,
                    },
                    max_source_read_bytes: 7,
                },
            },
        )
        .expect("historical semantic streaming");

        assert_eq!(actual, expected.bytes);
        assert_eq!(report.plan.reachable, vec![1, 2, 3, 4]);
        assert_eq!(report.plan.orphaned, vec![5]);
        assert_eq!(report.plan.edges_visited, 3);
        assert_eq!(report.plan.maximum_depth, 2);
        assert_eq!(report.output.selected_prefix_len, prefix_len);
        assert_eq!(report.output.output.selected_object_ids, vec![1, 2, 3, 4]);
        assert_eq!(report.output.output.output.output.report, expected.output);
        assert_eq!(
            report.output.output.output.cumulative_source_stats.bytes_read
                - report.output.output.output.inventory_stats.bytes_read,
            u64::try_from(
                b"root-at-one".len()
                    + b"dependency-two".len()
                    + b"dependency-three".len()
                    + b"dependency-four".len()
            )
            .expect("payload bytes")
        );
        assert!(source.largest_request <= 31);
    }

    #[test]
    fn graph_failure_touches_neither_source_nor_sink() {
        let format = ImmutableLimits::default();
        let mut source = VersionedMemorySource {
            data: source_file(format),
            version: [71; 32],
            reads: 0,
            largest_request: 0,
        };
        let mut invalid = ObjectGraph::new();
        invalid.add_object(1, vec![99]).expect("root");
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_compacted_versioned_source_sequence_to(
                &mut sink,
                &mut source,
                &invalid,
                &[1],
                1,
                ImmutableHistoricalSemanticStreamingOptions::default(),
            ),
            Err(ImmutableHistoricalSemanticStreamingError::Compaction(
                CompactionError::MissingObject(99)
            ))
        );
        assert_eq!(source.reads, 0);
        assert!(sink.is_empty());
    }

    #[test]
    fn reachable_object_missing_from_history_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let mut source = VersionedMemorySource {
            data: source_file(format),
            version: [73; 32],
            reads: 0,
            largest_request: 0,
        };
        let mut invalid = graph();
        invalid.add_object(99, Vec::new()).expect("external object");
        let mut sink = Vec::new();
        assert!(matches!(
            rewrite_compacted_versioned_source_sequence_to(
                &mut sink,
                &mut source,
                &invalid,
                &[99],
                1,
                ImmutableHistoricalSemanticStreamingOptions::default(),
            ),
            Err(ImmutableHistoricalSemanticStreamingError::Streaming(_))
        ));
        assert!(source.reads > 0);
        assert!(sink.is_empty());
    }
}
