use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_depth: usize,
}

impl Default for CompactionLimits {
    fn default() -> Self {
        Self {
            max_nodes: 1_000_000,
            max_edges: 4_000_000,
            max_depth: 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionError {
    DuplicateObject(u64),
    MissingObject(u64),
    NodeLimitExceeded,
    EdgeLimitExceeded,
    DepthLimitExceeded,
}

impl fmt::Display for CompactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateObject(id) => write!(f, "duplicate object {id}"),
            Self::MissingObject(id) => write!(f, "missing object {id}"),
            Self::NodeLimitExceeded => write!(f, "compaction node limit exceeded"),
            Self::EdgeLimitExceeded => write!(f, "compaction edge limit exceeded"),
            Self::DepthLimitExceeded => write!(f, "compaction dependency depth exceeded"),
        }
    }
}

impl std::error::Error for CompactionError {}

/// Logical dependency graph used to research reachability-based compaction.
#[derive(Debug, Clone, Default)]
pub struct ObjectGraph {
    dependencies: BTreeMap<u64, Vec<u64>>,
}

impl ObjectGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_object(
        &mut self,
        object_id: u64,
        dependencies: Vec<u64>,
    ) -> Result<(), CompactionError> {
        if self.dependencies.insert(object_id, dependencies).is_some() {
            return Err(CompactionError::DuplicateObject(object_id));
        }
        Ok(())
    }

    #[must_use]
    pub fn object_count(&self) -> usize {
        self.dependencies.len()
    }

    pub fn plan(
        &self,
        selected_roots: &[u64],
        limits: CompactionLimits,
    ) -> Result<CompactionPlan, CompactionError> {
        let mut reachable = BTreeSet::new();
        let mut stack = Vec::new();
        for &root in selected_roots.iter().rev() {
            if !self.dependencies.contains_key(&root) {
                return Err(CompactionError::MissingObject(root));
            }
            stack.push((root, 0_usize));
        }

        let mut edges_visited = 0_usize;
        let mut maximum_depth = 0_usize;
        while let Some((object_id, depth)) = stack.pop() {
            if depth > limits.max_depth {
                return Err(CompactionError::DepthLimitExceeded);
            }
            maximum_depth = maximum_depth.max(depth);
            if reachable.contains(&object_id) {
                continue;
            }
            if reachable.len() >= limits.max_nodes {
                return Err(CompactionError::NodeLimitExceeded);
            }
            reachable.insert(object_id);

            let dependencies = self
                .dependencies
                .get(&object_id)
                .ok_or(CompactionError::MissingObject(object_id))?;
            edges_visited = edges_visited
                .checked_add(dependencies.len())
                .ok_or(CompactionError::EdgeLimitExceeded)?;
            if edges_visited > limits.max_edges {
                return Err(CompactionError::EdgeLimitExceeded);
            }

            let next_depth = depth
                .checked_add(1)
                .ok_or(CompactionError::DepthLimitExceeded)?;
            for &dependency in dependencies.iter().rev() {
                if !self.dependencies.contains_key(&dependency) {
                    return Err(CompactionError::MissingObject(dependency));
                }
                if !reachable.contains(&dependency) {
                    stack.push((dependency, next_depth));
                }
            }
        }

        let all: BTreeSet<_> = self.dependencies.keys().copied().collect();
        let orphaned = all.difference(&reachable).copied().collect();
        Ok(CompactionPlan {
            selected_roots: selected_roots.to_vec(),
            reachable: reachable.into_iter().collect(),
            orphaned,
            edges_visited,
            maximum_depth,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionPlan {
    pub selected_roots: Vec<u64>,
    pub reachable: Vec<u64>,
    pub orphaned: Vec<u64>,
    pub edges_visited: usize,
    pub maximum_depth: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn plan_separates_reachable_objects_from_orphans() {
        let plan = graph()
            .plan(&[1], CompactionLimits::default())
            .expect("compaction plan");
        assert_eq!(plan.reachable, vec![1, 2, 3, 4]);
        assert_eq!(plan.orphaned, vec![5]);
        assert_eq!(plan.edges_visited, 3);
        assert_eq!(plan.maximum_depth, 2);
    }

    #[test]
    fn cycles_terminate_through_visited_identity_tracking() {
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![2]).expect("object 1");
        graph.add_object(2, vec![3]).expect("object 2");
        graph.add_object(3, vec![1]).expect("object 3");
        let plan = graph
            .plan(&[1], CompactionLimits::default())
            .expect("cyclic graph plan");
        assert_eq!(plan.reachable, vec![1, 2, 3]);
        assert!(plan.orphaned.is_empty());
    }

    #[test]
    fn missing_dependency_fails_closed() {
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![9]).expect("object 1");
        let error = graph
            .plan(&[1], CompactionLimits::default())
            .expect_err("missing dependency");
        assert_eq!(error, CompactionError::MissingObject(9));
    }

    #[test]
    fn node_edge_and_depth_limits_are_independent() {
        let graph = graph();
        assert_eq!(
            graph.plan(
                &[1],
                CompactionLimits {
                    max_nodes: 2,
                    ..CompactionLimits::default()
                }
            ),
            Err(CompactionError::NodeLimitExceeded)
        );
        assert_eq!(
            graph.plan(
                &[1],
                CompactionLimits {
                    max_edges: 2,
                    ..CompactionLimits::default()
                }
            ),
            Err(CompactionError::EdgeLimitExceeded)
        );
        assert_eq!(
            graph.plan(
                &[1],
                CompactionLimits {
                    max_depth: 1,
                    ..CompactionLimits::default()
                }
            ),
            Err(CompactionError::DepthLimitExceeded)
        );
    }

    #[test]
    fn duplicate_object_is_rejected_at_graph_construction() {
        let mut graph = ObjectGraph::new();
        graph.add_object(1, Vec::new()).expect("first object");
        assert_eq!(
            graph.add_object(1, Vec::new()),
            Err(CompactionError::DuplicateObject(1))
        );
    }
}
