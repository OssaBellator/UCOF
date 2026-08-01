use std::fmt;
use std::io::Write;

use crate::immutable_successor::{
    ImmutableLimits, ImmutableSourceStreamingWriteError, ImmutableSourceStreamingWriteOptions,
};
use crate::{
    rewrite_selected_active_file_to, CompactionError, CompactionLimits, CompactionPlan,
    ImmutableSelectedActiveStreamingReport, ObjectGraph,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSemanticStreamingError {
    Compaction(CompactionError),
    Streaming(ImmutableSourceStreamingWriteError),
}

impl fmt::Display for ImmutableSemanticStreamingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compaction(error) => write!(formatter, "semantic compaction failed: {error}"),
            Self::Streaming(error) => write!(formatter, "semantic streaming failed: {error}"),
        }
    }
}

impl std::error::Error for ImmutableSemanticStreamingError {}

impl From<CompactionError> for ImmutableSemanticStreamingError {
    fn from(error: CompactionError) -> Self {
        Self::Compaction(error)
    }
}

impl From<ImmutableSourceStreamingWriteError> for ImmutableSemanticStreamingError {
    fn from(error: ImmutableSourceStreamingWriteError) -> Self {
        Self::Streaming(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSemanticStreamingReport {
    pub plan: CompactionPlan,
    pub output: ImmutableSelectedActiveStreamingReport,
}

/// Computes bounded dependency closure and streams exactly the reachable active objects into a new
/// canonical genesis file.
///
/// The caller supplies the semantic dependency graph; this function does not infer references from
/// opaque payload bytes. Graph planning completes before source validation or sink output. The
/// selected active-state writer then validates the complete source file and emits only reachable
/// payloads. Orphaned objects and inactive history are not read by the output pass.
pub fn rewrite_compacted_active_file_to<W: Write>(
    writer: &mut W,
    data: &[u8],
    graph: &ObjectGraph,
    selected_roots: &[u64],
    compaction_limits: CompactionLimits,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
) -> Result<ImmutableSemanticStreamingReport, ImmutableSemanticStreamingError> {
    let plan = graph.plan(selected_roots, compaction_limits)?;
    let output = rewrite_selected_active_file_to(
        writer,
        data,
        &plan.reachable,
        options,
        limits,
    )?;
    Ok(ImmutableSemanticStreamingReport { plan, output })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immutable_successor::{
        append_replacement, build_genesis, rewrite_selected, ImmutableObjectInput,
        ImmutableStreamingWriteOptions,
    };

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

    #[test]
    fn dependency_closure_streams_only_reachable_active_payloads() {
        let limits = ImmutableLimits::default();
        let genesis = build_genesis(
            &[
                object(1, 11),
                object(2, 13),
                object(3, 17),
                object(4, 19),
                object(5, 4_096),
            ],
            limits,
        )
        .expect("genesis");
        let source = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(1, 77, b"active-one".to_vec()),
            limits,
        )
        .expect("replacement");
        let expected = rewrite_selected(&source, &[1, 2, 3, 4], limits).expect("selected rewrite");

        let mut actual = Vec::new();
        let report = rewrite_compacted_active_file_to(
            &mut actual,
            &source,
            &graph(),
            &[1],
            CompactionLimits::default(),
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 113,
                },
                max_source_read_bytes: 7,
            },
            limits,
        )
        .expect("semantic streaming");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.plan.reachable, vec![1, 2, 3, 4]);
        assert_eq!(report.plan.orphaned, vec![5]);
        assert_eq!(report.plan.edges_visited, 3);
        assert_eq!(report.plan.maximum_depth, 2);
        assert_eq!(report.output.output.source_bytes_read, 10 + 13 + 17 + 19);
        assert!(report.output.largest_payload_read_request <= 7);
        assert!(report.output.output.output.largest_write_request <= 113);
    }

    #[test]
    fn graph_failures_leave_sink_untouched() {
        let limits = ImmutableLimits::default();
        let source = build_genesis(&[object(1, 8)], limits).expect("genesis");
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![2]).expect("object 1");
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_compacted_active_file_to(
                &mut sink,
                &source,
                &graph,
                &[1],
                CompactionLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
                limits,
            ),
            Err(ImmutableSemanticStreamingError::Compaction(
                CompactionError::MissingObject(2)
            ))
        );
        assert!(sink.is_empty());
    }

    #[test]
    fn reachable_object_missing_from_source_leaves_sink_untouched() {
        let limits = ImmutableLimits::default();
        let source = build_genesis(&[object(1, 8)], limits).expect("genesis");
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![2]).expect("object 1");
        graph.add_object(2, Vec::new()).expect("object 2");
        let mut sink = Vec::new();
        assert!(matches!(
            rewrite_compacted_active_file_to(
                &mut sink,
                &source,
                &graph,
                &[1],
                CompactionLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
                limits,
            ),
            Err(ImmutableSemanticStreamingError::Streaming(_))
        ));
        assert!(sink.is_empty());
    }
}
