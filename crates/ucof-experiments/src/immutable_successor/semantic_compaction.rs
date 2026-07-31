use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableCompactionLimits {
    pub max_roots: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_depth: usize,
}

impl Default for ImmutableCompactionLimits {
    fn default() -> Self {
        Self {
            max_roots: 4_096,
            max_nodes: 1_000_000,
            max_edges: 4_000_000,
            max_depth: 1_024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownDependencyPolicy {
    /// Stop rather than risk dropping a dependency hidden behind unknown semantics.
    Reject,
    /// Retain the entire strictly validated active object set.
    ///
    /// Retaining only objects whose semantics are unknown is insufficient because those objects may
    /// depend on otherwise unselected known objects.
    RetainAllActive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyResolution {
    Known(Vec<u64>),
    Unknown,
}

/// Profile or application contract for logical object dependencies.
pub trait ImmutableDependencyResolver {
    fn dependencies(
        &mut self,
        object_id: u64,
        kind: u16,
        payload: &[u8],
    ) -> Result<DependencyResolution, &'static str>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableCompactionError {
    Format(ImmutableError),
    InvalidSelection,
    MissingDependency { object_id: u64, dependency_id: u64 },
    UnknownSemantics(u64),
    Resolver { object_id: u64, label: &'static str },
    Limit(&'static str),
}

impl fmt::Display for ImmutableCompactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "{error}"),
            Self::InvalidSelection => write!(formatter, "invalid compaction root selection"),
            Self::MissingDependency {
                object_id,
                dependency_id,
            } => write!(
                formatter,
                "object {object_id} depends on missing object {dependency_id}"
            ),
            Self::UnknownSemantics(object_id) => {
                write!(formatter, "unknown dependency semantics for object {object_id}")
            }
            Self::Resolver { object_id, label } => {
                write!(formatter, "dependency resolver failed for object {object_id}: {label}")
            }
            Self::Limit(label) => write!(formatter, "semantic compaction {label} limit exceeded"),
        }
    }
}

impl Error for ImmutableCompactionError {}

impl From<ImmutableError> for ImmutableCompactionError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSemanticCompactionResult {
    pub rewrite: ImmutableRewriteResult,
    pub selected_roots: Vec<u64>,
    pub retained_object_ids: Vec<u64>,
    pub discarded_object_ids: Vec<u64>,
    pub edges_visited: usize,
    pub maximum_depth: usize,
    pub unknown_semantics_trigger: Option<u64>,
    pub conservative_full_retention: bool,
}

fn locator_payload<'a>(data: &'a [u8], locator: &Locator) -> Result<&'a [u8], ImmutableError> {
    let offset = usize_from_u64(locator.record_offset, "compaction object")?;
    let length = usize_from_u64(locator.record_len, "compaction object")?;
    let record = checked_range(data, offset, length, "compaction object")?;
    let payload_length = length
        .checked_sub(OBJECT_HEADER_LEN)
        .ok_or(ImmutableError::Invalid("compaction object"))?;
    checked_range(
        record,
        OBJECT_HEADER_LEN,
        payload_length,
        "compaction object",
    )
}

/// Strictly validates an active source, resolves logical dependencies, and rewrites the retained
/// set into a new genesis file.
///
/// This operation makes a semantic-compaction claim only relative to the supplied resolver and
/// unknown-semantics policy. It does not retain historical snapshots, preserve byte-scoped
/// signatures, or infer dependencies from physical adjacency.
pub fn semantic_compact<R: ImmutableDependencyResolver>(
    data: &[u8],
    selected_roots: &[u64],
    resolver: &mut R,
    unknown_policy: UnknownDependencyPolicy,
    compaction_limits: ImmutableCompactionLimits,
    format_limits: ImmutableLimits,
) -> Result<ImmutableSemanticCompactionResult, ImmutableCompactionError> {
    if selected_roots.is_empty() || selected_roots.len() > compaction_limits.max_roots {
        return Err(ImmutableCompactionError::InvalidSelection);
    }

    let source = validate_internal(data, format_limits)?;
    allocation_check::<u64>(selected_roots.len(), format_limits)?;
    let mut roots = selected_roots.to_vec();
    roots.sort_unstable();
    roots.dedup();
    if roots.len() > compaction_limits.max_roots {
        return Err(ImmutableCompactionError::Limit("root count"));
    }

    let all_ids: Vec<u64> = source
        .locators
        .iter()
        .map(|locator| locator.object_id)
        .collect();
    allocation_check::<u64>(all_ids.len(), format_limits)?;
    for root in &roots {
        if source
            .locators
            .binary_search_by_key(root, |locator| locator.object_id)
            .is_err()
        {
            return Err(ImmutableCompactionError::Format(
                ImmutableError::MissingObject(*root),
            ));
        }
    }

    let mut retained = BTreeSet::new();
    let mut stack: Vec<(u64, usize)> = roots
        .iter()
        .rev()
        .map(|object_id| (*object_id, 0_usize))
        .collect();
    let mut edges_visited = 0_usize;
    let mut maximum_depth = 0_usize;
    let mut unknown_semantics_trigger = None;
    let mut conservative_full_retention = false;

    while let Some((object_id, depth)) = stack.pop() {
        if depth > compaction_limits.max_depth {
            return Err(ImmutableCompactionError::Limit("dependency depth"));
        }
        maximum_depth = maximum_depth.max(depth);
        if retained.contains(&object_id) {
            continue;
        }
        if retained.len() >= compaction_limits.max_nodes {
            return Err(ImmutableCompactionError::Limit("node count"));
        }

        let index = source
            .locators
            .binary_search_by_key(&object_id, |locator| locator.object_id)
            .map_err(|_| ImmutableCompactionError::Format(ImmutableError::MissingObject(object_id)))?;
        let locator = &source.locators[index];
        let payload = locator_payload(data, locator)?;
        retained.insert(object_id);

        match resolver
            .dependencies(object_id, locator.kind, payload)
            .map_err(|label| ImmutableCompactionError::Resolver { object_id, label })?
        {
            DependencyResolution::Known(mut dependencies) => {
                dependencies.sort_unstable();
                dependencies.dedup();
                edges_visited = edges_visited
                    .checked_add(dependencies.len())
                    .ok_or(ImmutableCompactionError::Limit("edge count"))?;
                if edges_visited > compaction_limits.max_edges {
                    return Err(ImmutableCompactionError::Limit("edge count"));
                }
                let next_depth = depth
                    .checked_add(1)
                    .ok_or(ImmutableCompactionError::Limit("dependency depth"))?;
                for dependency_id in dependencies.into_iter().rev() {
                    if source
                        .locators
                        .binary_search_by_key(&dependency_id, |entry| entry.object_id)
                        .is_err()
                    {
                        return Err(ImmutableCompactionError::MissingDependency {
                            object_id,
                            dependency_id,
                        });
                    }
                    if !retained.contains(&dependency_id) {
                        stack.push((dependency_id, next_depth));
                    }
                }
            }
            DependencyResolution::Unknown => match unknown_policy {
                UnknownDependencyPolicy::Reject => {
                    return Err(ImmutableCompactionError::UnknownSemantics(object_id));
                }
                UnknownDependencyPolicy::RetainAllActive => {
                    unknown_semantics_trigger = Some(object_id);
                    conservative_full_retention = true;
                    retained = all_ids.iter().copied().collect();
                    break;
                }
            },
        }
    }

    let retained_object_ids: Vec<u64> = retained.iter().copied().collect();
    if retained_object_ids.len() > compaction_limits.max_nodes {
        return Err(ImmutableCompactionError::Limit("node count"));
    }
    let discarded_object_ids = all_ids
        .iter()
        .copied()
        .filter(|object_id| !retained.contains(object_id))
        .collect();
    let rewrite = rewrite_selected(data, &retained_object_ids, format_limits)?;

    Ok(ImmutableSemanticCompactionResult {
        rewrite,
        selected_roots: roots,
        retained_object_ids,
        discarded_object_ids,
        edges_visited,
        maximum_depth,
        unknown_semantics_trigger,
        conservative_full_retention,
    })
}
