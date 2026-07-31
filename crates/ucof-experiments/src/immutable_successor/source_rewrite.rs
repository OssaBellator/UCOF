use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceRewriteReport {
    pub rewrite: ImmutableRewriteResult,
    pub stats: ImmutableSourceStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceSemanticCompactionReport {
    pub compaction: ImmutableSemanticCompactionResult,
    pub stats: ImmutableSourceStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSourceCompactionError {
    Source(ImmutableSourceError),
    Compaction(ImmutableCompactionError),
}

impl fmt::Display for ImmutableSourceCompactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "{error}"),
            Self::Compaction(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for ImmutableSourceCompactionError {}

impl From<ImmutableSourceError> for ImmutableSourceCompactionError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

impl From<ImmutableCompactionError> for ImmutableSourceCompactionError {
    fn from(error: ImmutableCompactionError) -> Self {
        Self::Compaction(error)
    }
}

struct ImmutableSourceInventory {
    report: ImmutableReport,
    locators: Vec<Locator>,
    stats: ImmutableSourceStats,
}

fn validated_source_inventory<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceInventory, ImmutableSourceError> {
    let strict = validate_source_at(source, limits)?;
    let remaining = remaining_source_limits(limits, strict.stats)?;
    let mut reader = SourceReader::new(source, remaining)?;
    let envelope = read_lookup_envelope(&mut reader)?;
    if envelope.sequence != strict.report.sequence
        || envelope.snapshot_digest != strict.report.snapshot_digest
        || envelope.commit_digest != strict.report.commit_digest
        || envelope.root.level != strict.report.root_level
    {
        return Err(ImmutableSourceError::Io("source changed"));
    }

    let mut visited = HashSet::new();
    let mut stack = vec![envelope.root.clone()];
    let mut locators = Vec::new();
    let mut known_ranges = vec![
        (envelope.snapshot_offset, envelope.footer_offset),
        (envelope.footer_offset, reader.length),
    ];
    while let Some(reference) = stack.pop() {
        read_full_page(
            &mut reader,
            &reference,
            &envelope,
            &mut visited,
            &mut stack,
            &mut locators,
            &mut known_ranges,
        )?;
    }
    locators.sort_by_key(|locator| locator.object_id);
    if locators.len() != strict.report.object_count
        || visited.len() != strict.report.page_count
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableSourceError::Io("source changed"));
    }

    let mut stats = strict.stats;
    add_source_stats(&mut stats, reader.stats)?;
    Ok(ImmutableSourceInventory {
        report: strict.report,
        locators,
        stats,
    })
}

fn source_input_from_locator<S: ImmutableReadAt>(
    source: &mut S,
    locator: &Locator,
    limits: ImmutableSourceLimits,
    stats: &mut ImmutableSourceStats,
) -> Result<ImmutableObjectInput, ImmutableSourceError> {
    let length = usize_from_u64(locator.record_len, "rewrite object")?;
    if length < OBJECT_HEADER_LEN || length > limits.format.max_allocation_bytes {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit(
            "allocation",
        )));
    }
    stats.largest_allocation = stats.largest_allocation.max(length);
    let mut record = vec![0_u8; length];
    read_direct(
        source,
        limits,
        stats,
        locator.record_offset,
        &mut record,
    )?;
    if &record[..8] != OBJECT_MAGIC
        || usize::from(u16_at(&record, 8, "rewrite object")?) != OBJECT_HEADER_LEN
        || u32_at(&record, 12, "rewrite object")? != 0
        || record[40..OBJECT_HEADER_LEN]
            .iter()
            .any(|byte| *byte != 0)
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "rewrite object",
        )));
    }
    let object_id = u64_at(&record, 16, "rewrite object")?;
    let kind = u16_at(&record, 10, "rewrite object")?;
    let payload_len = usize_at(&record, 24, "rewrite object")?;
    let logical_len = u64_at(&record, 32, "rewrite object")?;
    if object_id != locator.object_id
        || kind != locator.kind
        || kind == 0
        || OBJECT_HEADER_LEN
            .checked_add(payload_len)
            .is_none_or(|value| value != length)
        || u64_from_usize(payload_len)? != logical_len
        || logical_len != locator.logical_len
        || digest(&[OBJECT_DOMAIN, &record]) != locator.digest
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "rewrite object",
        )));
    }
    stats.bytes_hashed = stats
        .bytes_hashed
        .checked_add(
            u64::try_from(record.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    Ok(ImmutableObjectInput::new(
        object_id,
        kind,
        record[OBJECT_HEADER_LEN..].to_vec(),
    ))
}

fn canonical_selected_locators<'a>(
    inventory: &'a ImmutableSourceInventory,
    requested_ids: &[u64],
    limits: ImmutableLimits,
) -> Result<(Vec<u64>, Vec<&'a Locator>), ImmutableSourceError> {
    if requested_ids.is_empty() {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "rewrite selection",
        )));
    }
    allocation_check::<u64>(requested_ids.len(), limits)?;
    allocation_check::<&Locator>(requested_ids.len(), limits)?;
    let mut retained_object_ids = requested_ids.to_vec();
    retained_object_ids.sort_unstable();
    if retained_object_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "rewrite selection",
        )));
    }
    if retained_object_ids.len() > limits.max_objects {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit(
            "object count",
        )));
    }
    let mut selected = Vec::with_capacity(retained_object_ids.len());
    for object_id in &retained_object_ids {
        let index = inventory
            .locators
            .binary_search_by_key(object_id, |locator| locator.object_id)
            .map_err(|_| {
                ImmutableSourceError::Format(ImmutableError::MissingObject(*object_id))
            })?;
        selected.push(&inventory.locators[index]);
    }
    check_rewrite_allocation(&selected, limits)?;
    Ok((retained_object_ids, selected))
}

fn rewrite_source_from_inventory<S: ImmutableReadAt>(
    source: &mut S,
    requested_ids: &[u64],
    inventory: ImmutableSourceInventory,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceRewriteReport, ImmutableSourceError> {
    let (retained_object_ids, selected) =
        canonical_selected_locators(&inventory, requested_ids, limits.format)?;
    let mut stats = inventory.stats;
    let mut inputs = Vec::with_capacity(selected.len());
    for locator in selected {
        inputs.push(source_input_from_locator(source, locator, limits, &mut stats)?);
    }
    let bytes = build_genesis(&inputs, limits.format)?;
    let output = validate(&bytes, limits.format)?;
    Ok(ImmutableSourceRewriteReport {
        rewrite: ImmutableRewriteResult {
            bytes,
            source: inventory.report,
            output,
            retained_object_ids,
            byte_scoped_signatures_preserved: false,
        },
        stats,
    })
}

/// Strictly validates a bounded random-access source and rewrites all active objects without
/// materializing the complete source file.
///
/// Active object payloads are materialized because the current deterministic genesis writer accepts
/// owned inputs. Source bytes outside the active selected records are never copied into one whole-file
/// buffer.
pub fn rewrite_source_all<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceRewriteReport, ImmutableSourceError> {
    let inventory = validated_source_inventory(source, limits)?;
    allocation_check::<u64>(inventory.locators.len(), limits.format)?;
    let ids: Vec<u64> = inventory
        .locators
        .iter()
        .map(|locator| locator.object_id)
        .collect();
    rewrite_source_from_inventory(source, &ids, inventory, limits)
}

/// Strictly validates a bounded random-access source and rewrites caller-selected active objects.
///
/// This function performs no dependency discovery and therefore has the same lower semantic
/// assurance as slice-based `rewrite_selected`.
pub fn rewrite_source_selected<S: ImmutableReadAt>(
    source: &mut S,
    object_ids: &[u64],
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceRewriteReport, ImmutableSourceError> {
    let inventory = validated_source_inventory(source, limits)?;
    rewrite_source_from_inventory(source, object_ids, inventory, limits)
}

/// Strictly validates a bounded random-access source, resolves active logical dependencies, and
/// rewrites the retained set without materializing the complete source file.
///
/// Dependency traversal reads one object record at a time. Retained payloads are read again for the
/// current owned-input genesis writer, so callers must budget both traversal and rewrite reads.
pub fn semantic_compact_source<S: ImmutableReadAt, R: ImmutableDependencyResolver>(
    source: &mut S,
    selected_roots: &[u64],
    resolver: &mut R,
    unknown_policy: UnknownDependencyPolicy,
    compaction_limits: ImmutableCompactionLimits,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceSemanticCompactionReport, ImmutableSourceCompactionError> {
    if selected_roots.is_empty() || selected_roots.len() > compaction_limits.max_roots {
        return Err(ImmutableCompactionError::InvalidSelection.into());
    }
    let inventory = validated_source_inventory(source, limits)?;
    allocation_check::<u64>(selected_roots.len(), limits.format)?;
    let mut roots = selected_roots.to_vec();
    roots.sort_unstable();
    roots.dedup();

    allocation_check::<u64>(inventory.locators.len(), limits.format)?;
    let all_ids: Vec<u64> = inventory
        .locators
        .iter()
        .map(|locator| locator.object_id)
        .collect();
    for root in &roots {
        if inventory
            .locators
            .binary_search_by_key(root, |locator| locator.object_id)
            .is_err()
        {
            return Err(ImmutableCompactionError::Format(
                ImmutableError::MissingObject(*root),
            )
            .into());
        }
    }

    let mut stats = inventory.stats;
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
            return Err(ImmutableCompactionError::Limit("dependency depth").into());
        }
        maximum_depth = maximum_depth.max(depth);
        if retained.contains(&object_id) {
            continue;
        }
        if retained.len() >= compaction_limits.max_nodes {
            return Err(ImmutableCompactionError::Limit("node count").into());
        }
        let index = inventory
            .locators
            .binary_search_by_key(&object_id, |locator| locator.object_id)
            .map_err(|_| {
                ImmutableSourceCompactionError::Compaction(
                    ImmutableCompactionError::Format(ImmutableError::MissingObject(object_id)),
                )
            })?;
        let locator = &inventory.locators[index];
        let input = source_input_from_locator(source, locator, limits, &mut stats)?;
        retained.insert(object_id);
        match resolver
            .dependencies(object_id, input.kind, &input.payload)
            .map_err(|label| {
                ImmutableSourceCompactionError::Compaction(
                    ImmutableCompactionError::Resolver { object_id, label },
                )
            })? {
            DependencyResolution::Known(mut dependencies) => {
                dependencies.sort_unstable();
                dependencies.dedup();
                edges_visited = edges_visited.checked_add(dependencies.len()).ok_or(
                    ImmutableSourceCompactionError::Compaction(
                        ImmutableCompactionError::Limit("edge count"),
                    ),
                )?;
                if edges_visited > compaction_limits.max_edges {
                    return Err(ImmutableCompactionError::Limit("edge count").into());
                }
                let next_depth = depth
                    .checked_add(1)
                    .ok_or(ImmutableCompactionError::Limit("dependency depth"))?;
                for dependency_id in dependencies.into_iter().rev() {
                    if inventory
                        .locators
                        .binary_search_by_key(&dependency_id, |entry| entry.object_id)
                        .is_err()
                    {
                        return Err(ImmutableCompactionError::MissingDependency {
                            object_id,
                            dependency_id,
                        }
                        .into());
                    }
                    if !retained.contains(&dependency_id) {
                        stack.push((dependency_id, next_depth));
                    }
                }
            }
            DependencyResolution::Unknown => match unknown_policy {
                UnknownDependencyPolicy::Reject => {
                    return Err(ImmutableCompactionError::UnknownSemantics(object_id).into());
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
        return Err(ImmutableCompactionError::Limit("node count").into());
    }
    let discarded_object_ids: Vec<u64> = all_ids
        .iter()
        .copied()
        .filter(|object_id| !retained.contains(object_id))
        .collect();
    let inventory = ImmutableSourceInventory {
        report: inventory.report,
        locators: inventory.locators,
        stats,
    };
    let rewritten = rewrite_source_from_inventory(source, &retained_object_ids, inventory, limits)?;
    Ok(ImmutableSourceSemanticCompactionReport {
        compaction: ImmutableSemanticCompactionResult {
            rewrite: rewritten.rewrite,
            selected_roots: roots,
            retained_object_ids,
            discarded_object_ids,
            edges_visited,
            maximum_depth,
            unknown_semantics_trigger,
            conservative_full_retention,
        },
        stats: rewritten.stats,
    })
}
