use std::collections::BTreeMap;

use ucof_experiments::immutable_successor::{
    build_genesis, rewrite_all, semantic_compact, DependencyResolution,
    ImmutableCompactionError, ImmutableCompactionLimits, ImmutableDependencyResolver,
    ImmutableError, ImmutableLimits, ImmutableObjectInput, UnknownDependencyPolicy,
};

#[derive(Default)]
struct Resolver {
    dependencies: BTreeMap<u64, DependencyResolution>,
    failures: BTreeMap<u64, &'static str>,
}

impl Resolver {
    fn with(mut self, object_id: u64, resolution: DependencyResolution) -> Self {
        self.dependencies.insert(object_id, resolution);
        self
    }

    fn failing(mut self, object_id: u64, label: &'static str) -> Self {
        self.failures.insert(object_id, label);
        self
    }
}

impl ImmutableDependencyResolver for Resolver {
    fn dependencies(
        &mut self,
        object_id: u64,
        _kind: u16,
        _payload: &[u8],
    ) -> Result<DependencyResolution, &'static str> {
        if let Some(label) = self.failures.get(&object_id) {
            return Err(label);
        }
        Ok(self
            .dependencies
            .get(&object_id)
            .cloned()
            .unwrap_or(DependencyResolution::Known(Vec::new())))
    }
}

fn source() -> Vec<u8> {
    let objects: Vec<_> = (1_u64..=6)
        .map(|object_id| {
            ImmutableObjectInput::new(
                object_id,
                u16::try_from(object_id).expect("kind"),
                format!("object:{object_id}").into_bytes(),
            )
        })
        .collect();
    build_genesis(&objects, ImmutableLimits::default()).expect("genesis")
}

fn graph_resolver() -> Resolver {
    Resolver::default()
        .with(1, DependencyResolution::Known(vec![2, 3]))
        .with(2, DependencyResolution::Known(vec![4]))
        .with(3, DependencyResolution::Known(Vec::new()))
        .with(4, DependencyResolution::Known(Vec::new()))
        .with(5, DependencyResolution::Unknown)
        .with(6, DependencyResolution::Known(Vec::new()))
}

#[test]
fn semantic_compaction_rewrites_exact_reachable_set() {
    let bytes = source();
    let result = semantic_compact(
        &bytes,
        &[1],
        &mut graph_resolver(),
        UnknownDependencyPolicy::Reject,
        ImmutableCompactionLimits::default(),
        ImmutableLimits::default(),
    )
    .expect("semantic compaction");

    assert_eq!(result.selected_roots, vec![1]);
    assert_eq!(result.retained_object_ids, vec![1, 2, 3, 4]);
    assert_eq!(result.discarded_object_ids, vec![5, 6]);
    assert_eq!(result.edges_visited, 3);
    assert_eq!(result.maximum_depth, 2);
    assert!(!result.conservative_full_retention);
    assert_eq!(result.rewrite.output.object_count, 4);
    assert!(!result.rewrite.byte_scoped_signatures_preserved);
}

#[test]
fn unknown_semantics_abort_or_retain_the_entire_active_set() {
    let bytes = source();
    assert_eq!(
        semantic_compact(
            &bytes,
            &[5],
            &mut graph_resolver(),
            UnknownDependencyPolicy::Reject,
            ImmutableCompactionLimits::default(),
            ImmutableLimits::default(),
        ),
        Err(ImmutableCompactionError::UnknownSemantics(5))
    );

    let conservative = semantic_compact(
        &bytes,
        &[5],
        &mut graph_resolver(),
        UnknownDependencyPolicy::RetainAllActive,
        ImmutableCompactionLimits::default(),
        ImmutableLimits::default(),
    )
    .expect("conservative compaction");
    assert_eq!(conservative.retained_object_ids, vec![1, 2, 3, 4, 5, 6]);
    assert!(conservative.discarded_object_ids.is_empty());
    assert_eq!(conservative.unknown_semantics_trigger, Some(5));
    assert!(conservative.conservative_full_retention);
    assert_eq!(
        conservative.rewrite.bytes,
        rewrite_all(&bytes, ImmutableLimits::default())
            .expect("rewrite all")
            .bytes
    );
}

#[test]
fn cycles_terminate_and_missing_dependencies_fail_closed() {
    let bytes = source();
    let mut cyclic = Resolver::default()
        .with(1, DependencyResolution::Known(vec![2]))
        .with(2, DependencyResolution::Known(vec![3]))
        .with(3, DependencyResolution::Known(vec![1]));
    let result = semantic_compact(
        &bytes,
        &[1],
        &mut cyclic,
        UnknownDependencyPolicy::Reject,
        ImmutableCompactionLimits::default(),
        ImmutableLimits::default(),
    )
    .expect("cyclic compaction");
    assert_eq!(result.retained_object_ids, vec![1, 2, 3]);

    let mut missing = Resolver::default().with(1, DependencyResolution::Known(vec![99]));
    assert_eq!(
        semantic_compact(
            &bytes,
            &[1],
            &mut missing,
            UnknownDependencyPolicy::Reject,
            ImmutableCompactionLimits::default(),
            ImmutableLimits::default(),
        ),
        Err(ImmutableCompactionError::MissingDependency {
            object_id: 1,
            dependency_id: 99,
        })
    );
}

#[test]
fn resolver_and_work_limits_are_independent() {
    let bytes = source();
    let limits = ImmutableLimits::default();
    assert_eq!(
        semantic_compact(
            &bytes,
            &[1],
            &mut Resolver::default().failing(1, "profile failure"),
            UnknownDependencyPolicy::Reject,
            ImmutableCompactionLimits::default(),
            limits,
        ),
        Err(ImmutableCompactionError::Resolver {
            object_id: 1,
            label: "profile failure",
        })
    );

    for (compaction_limits, label) in [
        (
            ImmutableCompactionLimits {
                max_nodes: 2,
                ..ImmutableCompactionLimits::default()
            },
            "node count",
        ),
        (
            ImmutableCompactionLimits {
                max_edges: 2,
                ..ImmutableCompactionLimits::default()
            },
            "edge count",
        ),
        (
            ImmutableCompactionLimits {
                max_depth: 1,
                ..ImmutableCompactionLimits::default()
            },
            "dependency depth",
        ),
    ] {
        assert_eq!(
            semantic_compact(
                &bytes,
                &[1],
                &mut graph_resolver(),
                UnknownDependencyPolicy::Reject,
                compaction_limits,
                limits,
            ),
            Err(ImmutableCompactionError::Limit(label))
        );
    }

    assert_eq!(
        semantic_compact(
            &bytes,
            &[99],
            &mut graph_resolver(),
            UnknownDependencyPolicy::Reject,
            ImmutableCompactionLimits::default(),
            limits,
        ),
        Err(ImmutableCompactionError::Format(
            ImmutableError::MissingObject(99)
        ))
    );
}
