use std::fmt;

use crate::{CompactionError, CompactionLimits, CompactionPlan, ObjectGraph};

#[derive(Clone, Copy, Debug)]
pub struct HistoricalSemanticSelectionRequest<'a> {
    pub sequence: u64,
    pub graph: &'a ObjectGraph,
    pub selected_roots: &'a [u64],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoricalSemanticSelectionLimits {
    pub compaction: CompactionLimits,
    pub max_states: usize,
    pub max_total_reachable_objects: usize,
}

impl Default for HistoricalSemanticSelectionLimits {
    fn default() -> Self {
        Self {
            compaction: CompactionLimits::default(),
            max_states: 1_024,
            max_total_reachable_objects: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalSemanticSelectionEntry {
    pub sequence: u64,
    pub plan: CompactionPlan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalSemanticSelectionPlan {
    pub entries: Vec<HistoricalSemanticSelectionEntry>,
    pub total_reachable_objects: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoricalSemanticSelectionError {
    Empty,
    DuplicateSequence(u64),
    Limit(&'static str),
    Compaction {
        sequence: u64,
        error: CompactionError,
    },
}

impl fmt::Display for HistoricalSemanticSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "historical semantic selection is empty"),
            Self::DuplicateSequence(sequence) => {
                write!(
                    formatter,
                    "duplicate historical semantic sequence {sequence}"
                )
            }
            Self::Limit(label) => write!(formatter, "historical semantic limit exceeded: {label}"),
            Self::Compaction { sequence, error } => {
                write!(
                    formatter,
                    "historical semantic sequence {sequence} failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for HistoricalSemanticSelectionError {}

/// Plans dependency closure independently for each retained historical state.
///
/// Requests are canonicalized by sequence. Each request supplies its own graph and trusted roots;
/// no closure, orphan set, edge count, or depth fact is reused across states. The result is bounded
/// by both a state count and a cumulative reachable-object count. This is a planning layer only and
/// does not validate source history or emit a multi-snapshot output chain.
pub fn plan_historical_semantic_selections(
    requests: &[HistoricalSemanticSelectionRequest<'_>],
    limits: HistoricalSemanticSelectionLimits,
) -> Result<HistoricalSemanticSelectionPlan, HistoricalSemanticSelectionError> {
    if requests.is_empty() {
        return Err(HistoricalSemanticSelectionError::Empty);
    }
    if requests.len() > limits.max_states {
        return Err(HistoricalSemanticSelectionError::Limit("states"));
    }

    let mut order: Vec<usize> = (0..requests.len()).collect();
    order.sort_unstable_by_key(|index| requests[*index].sequence);
    if let Some(pair) = order
        .windows(2)
        .find(|pair| requests[pair[0]].sequence == requests[pair[1]].sequence)
    {
        return Err(HistoricalSemanticSelectionError::DuplicateSequence(
            requests[pair[0]].sequence,
        ));
    }

    let mut entries = Vec::with_capacity(requests.len());
    let mut total_reachable_objects = 0_usize;
    for index in order {
        let request = requests[index];
        let plan = request
            .graph
            .plan(request.selected_roots, limits.compaction)
            .map_err(|error| HistoricalSemanticSelectionError::Compaction {
                sequence: request.sequence,
                error,
            })?;
        total_reachable_objects = total_reachable_objects
            .checked_add(plan.reachable.len())
            .ok_or(HistoricalSemanticSelectionError::Limit(
                "total reachable objects",
            ))?;
        if total_reachable_objects > limits.max_total_reachable_objects {
            return Err(HistoricalSemanticSelectionError::Limit(
                "total reachable objects",
            ));
        }
        entries.push(HistoricalSemanticSelectionEntry {
            sequence: request.sequence,
            plan,
        });
    }

    Ok(HistoricalSemanticSelectionPlan {
        entries,
        total_reachable_objects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_graph() -> ObjectGraph {
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![2]).expect("one");
        graph.add_object(2, vec![3]).expect("two");
        graph.add_object(3, Vec::new()).expect("three");
        graph.add_object(4, Vec::new()).expect("four");
        graph
    }

    fn second_graph() -> ObjectGraph {
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![4]).expect("one");
        graph.add_object(2, Vec::new()).expect("two");
        graph.add_object(3, Vec::new()).expect("three");
        graph.add_object(4, Vec::new()).expect("four");
        graph
    }

    #[test]
    fn each_state_uses_its_own_graph_and_roots() {
        let first = first_graph();
        let second = second_graph();
        let plan = plan_historical_semantic_selections(
            &[
                HistoricalSemanticSelectionRequest {
                    sequence: 9,
                    graph: &second,
                    selected_roots: &[1],
                },
                HistoricalSemanticSelectionRequest {
                    sequence: 3,
                    graph: &first,
                    selected_roots: &[1],
                },
            ],
            HistoricalSemanticSelectionLimits::default(),
        )
        .expect("per-state plan");

        assert_eq!(plan.entries[0].sequence, 3);
        assert_eq!(plan.entries[0].plan.reachable, vec![1, 2, 3]);
        assert_eq!(plan.entries[0].plan.orphaned, vec![4]);
        assert_eq!(plan.entries[1].sequence, 9);
        assert_eq!(plan.entries[1].plan.reachable, vec![1, 4]);
        assert_eq!(plan.entries[1].plan.orphaned, vec![2, 3]);
        assert_eq!(plan.total_reachable_objects, 5);
    }

    #[test]
    fn duplicate_sequences_and_cumulative_limits_fail() {
        let graph = first_graph();
        let duplicate = [
            HistoricalSemanticSelectionRequest {
                sequence: 4,
                graph: &graph,
                selected_roots: &[1],
            },
            HistoricalSemanticSelectionRequest {
                sequence: 4,
                graph: &graph,
                selected_roots: &[4],
            },
        ];
        assert_eq!(
            plan_historical_semantic_selections(
                &duplicate,
                HistoricalSemanticSelectionLimits::default(),
            ),
            Err(HistoricalSemanticSelectionError::DuplicateSequence(4))
        );

        let requests = [
            HistoricalSemanticSelectionRequest {
                sequence: 1,
                graph: &graph,
                selected_roots: &[1],
            },
            HistoricalSemanticSelectionRequest {
                sequence: 2,
                graph: &graph,
                selected_roots: &[4],
            },
        ];
        assert_eq!(
            plan_historical_semantic_selections(
                &requests,
                HistoricalSemanticSelectionLimits {
                    max_total_reachable_objects: 3,
                    ..HistoricalSemanticSelectionLimits::default()
                },
            ),
            Err(HistoricalSemanticSelectionError::Limit(
                "total reachable objects"
            ))
        );
    }
}
