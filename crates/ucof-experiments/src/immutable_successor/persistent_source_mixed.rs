#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentSourceMixedError {
    Version(ImmutableSourceError),
    VersionChanged,
    Source(ImmutableSourceError),
    Writer(ImmutableError),
}

impl std::fmt::Display for PersistentSourceMixedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(error) => write!(formatter, "persistent source version failed: {error}"),
            Self::VersionChanged => write!(formatter, "persistent source version changed"),
            Self::Source(error) => write!(formatter, "persistent source planning failed: {error}"),
            Self::Writer(error) => write!(formatter, "persistent mixed tail failed: {error}"),
        }
    }
}

impl std::error::Error for PersistentSourceMixedError {}

impl From<ImmutableError> for PersistentSourceMixedError {
    fn from(error: ImmutableError) -> Self {
        Self::Writer(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentSourceMixedPlan {
    pub identity: PersistentSourceIdentity,
    pub version: PersistentSourceVersion,
    pub tail: Vec<u8>,
    pub report: ImmutableReport,
    pub pages_written: usize,
    pub pages_reused: usize,
    pub source_stats: ImmutableSourceStats,
    pub version_checks: u64,
    pub tail_allocation_bytes: usize,
    pub retained_metadata_allocation_bytes: usize,
}

struct PersistentSourceMixedInner {
    identity: PersistentSourceIdentity,
    tail: Vec<u8>,
    report: ImmutableReport,
    pages_written: usize,
    pages_reused: usize,
    source_stats: ImmutableSourceStats,
    retained_metadata_allocation_bytes: usize,
}

fn mixed_source_error(error: ImmutableSourceError) -> PersistentSourceMixedError {
    PersistentSourceMixedError::Source(error)
}

fn mixed_writer_error(error: ImmutableError) -> PersistentSourceMixedError {
    PersistentSourceMixedError::Writer(error)
}

fn collect_source_mixed_inventory<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    reference: &PageRef,
    levels: &mut [Vec<OriginalMixedPage>],
    locators: &mut Vec<Locator>,
    visited: &mut usize,
) -> Result<(), PersistentSourceMixedError> {
    if *visited >= reader.limits.format.max_pages {
        return Err(mixed_writer_error(ImmutableError::Limit("page count")));
    }
    *visited += 1;
    let node = read_source_deletion_node(reader, reference).map_err(|error| match error {
        PersistentSourceDeletionError::Source(error) => PersistentSourceMixedError::Source(error),
        PersistentSourceDeletionError::Writer(error) => PersistentSourceMixedError::Writer(error),
        PersistentSourceDeletionError::Version(error) => PersistentSourceMixedError::Version(error),
        PersistentSourceDeletionError::VersionChanged => PersistentSourceMixedError::VersionChanged,
    })?;
    let target = levels
        .get_mut(usize::from(reference.level))
        .ok_or_else(|| mixed_writer_error(ImmutableError::Invalid("mixed page level")))?;
    allocation_check::<OriginalMixedPage>(
        target
            .len()
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page count"))?,
        reader.limits.format,
    )
    .map_err(mixed_writer_error)?;

    match node {
        PendingDeletionNode::Leaf(entries) => {
            let next_count = locators
                .len()
                .checked_add(entries.len())
                .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("object count")))?;
            if next_count > reader.limits.format.max_objects {
                return Err(mixed_writer_error(ImmutableError::Limit("object count")));
            }
            allocation_check::<Locator>(next_count, reader.limits.format)
                .map_err(mixed_writer_error)?;
            locators.extend(entries.iter().cloned());
            target.push(OriginalMixedPage {
                reference: reference.clone(),
                body: OriginalMixedPageBody::Leaf(entries),
            });
        }
        PendingDeletionNode::Internal { level: _, children } => {
            target.push(OriginalMixedPage {
                reference: reference.clone(),
                body: OriginalMixedPageBody::Internal(children.clone()),
            });
            for child in &children {
                collect_source_mixed_inventory(reader, child, levels, locators, visited)?;
            }
        }
    }
    Ok(())
}

fn source_mixed_operation_order(
    operations: &[ImmutableBatchOperation],
    locators: &[Locator],
    limits: ImmutableLimits,
) -> Result<Vec<usize>, PersistentSourceMixedError> {
    if operations.len() < 2
        || !operations
            .iter()
            .any(|operation| matches!(operation, ImmutableBatchOperation::Delete(_)))
    {
        return Err(mixed_writer_error(ImmutableError::Invalid(
            "persistent mixed batch",
        )));
    }
    allocation_check::<usize>(operations.len(), limits).map_err(mixed_writer_error)?;
    let mut order: Vec<usize> = (0..operations.len()).collect();
    order.sort_unstable_by_key(|index| operations[*index].object_id());
    if let Some(pair) = order.windows(2).find(|pair| {
        operations[pair[0]].object_id() == operations[pair[1]].object_id()
    }) {
        return Err(mixed_writer_error(ImmutableError::DuplicateObject(
            operations[pair[0]].object_id(),
        )));
    }

    let mut insertions = 0_usize;
    let mut deletions = 0_usize;
    for index in &order {
        match &operations[*index] {
            ImmutableBatchOperation::Put(input) => {
                if input.object_id == 0 || input.kind == 0 {
                    return Err(mixed_writer_error(ImmutableError::Invalid(
                        "object input",
                    )));
                }
                if locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                    .is_err()
                {
                    insertions = insertions
                        .checked_add(1)
                        .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("object count")))?;
                }
            }
            ImmutableBatchOperation::Delete(object_id) => {
                if *object_id == 0
                    || locators
                        .binary_search_by_key(object_id, |locator| locator.object_id)
                        .is_err()
                {
                    return Err(mixed_writer_error(ImmutableError::MissingObject(*object_id)));
                }
                deletions = deletions
                    .checked_add(1)
                    .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("object count")))?;
            }
        }
    }
    let next_count = locators
        .len()
        .checked_add(insertions)
        .and_then(|count| count.checked_sub(deletions))
        .ok_or_else(|| mixed_writer_error(ImmutableError::Invalid("empty directory")))?;
    if next_count == 0 {
        return Err(mixed_writer_error(ImmutableError::Invalid("empty directory")));
    }
    if next_count > limits.max_objects {
        return Err(mixed_writer_error(ImmutableError::Limit("object count")));
    }
    allocation_check::<Locator>(next_count, limits).map_err(mixed_writer_error)?;
    Ok(order)
}

fn apply_source_mixed_operations(
    tail: &mut Vec<u8>,
    base_len: usize,
    operations: &[ImmutableBatchOperation],
    order: &[usize],
    mut locators: Vec<Locator>,
    limits: ImmutableLimits,
) -> Result<Vec<Locator>, PersistentSourceMixedError> {
    for index in order {
        match &operations[*index] {
            ImmutableBatchOperation::Put(input) => {
                let replacement = append_persistent_tail_object(tail, base_len, input, limits)
                    .map_err(mixed_writer_error)?;
                match locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                {
                    Ok(position) => locators[position] = replacement,
                    Err(position) => locators.insert(position, replacement),
                }
            }
            ImmutableBatchOperation::Delete(object_id) => {
                let position = locators
                    .binary_search_by_key(object_id, |locator| locator.object_id)
                    .map_err(|_| mixed_writer_error(ImmutableError::MissingObject(*object_id)))?;
                locators.remove(position);
            }
        }
    }
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(mixed_writer_error(ImmutableError::Invalid(
            "persistent mixed locator order",
        )));
    }
    Ok(locators)
}

fn mixed_metadata_allocation(
    locators: &Vec<Locator>,
    originals: &[Vec<OriginalMixedPage>],
) -> Result<usize, PersistentSourceMixedError> {
    let locator_bytes = locators
        .capacity()
        .checked_mul(std::mem::size_of::<Locator>())
        .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("allocation")))?;
    originals.iter().try_fold(locator_bytes, |total, level| {
        let page_bytes = level
            .capacity()
            .checked_mul(std::mem::size_of::<OriginalMixedPage>())
            .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("allocation")))?;
        total
            .checked_add(page_bytes)
            .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("allocation")))
    })
}

fn plan_persistent_source_mixed_inner<S: ImmutableReadAt>(
    source: &mut S,
    operations: &[ImmutableBatchOperation],
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceMixedInner, PersistentSourceMixedError> {
    let strict = validate_source_at(source, limits).map_err(mixed_source_error)?;
    let mut total_stats = strict.stats;

    let canonical_limits = remaining_source_limits(limits, total_stats).map_err(mixed_source_error)?;
    let (envelope, canonical_stats) =
        persistent_source_canonical_envelope(source, canonical_limits, &strict.report)
            .map_err(mixed_source_error)?;
    add_source_stats(&mut total_stats, canonical_stats).map_err(mixed_source_error)?;

    let identity_limits = remaining_source_limits(limits, total_stats).map_err(mixed_source_error)?;
    let (identity, identity_stats) =
        persistent_source_identity(source, identity_limits).map_err(mixed_source_error)?;
    add_source_stats(&mut total_stats, identity_stats).map_err(mixed_source_error)?;

    let inventory_limits = remaining_source_limits(limits, total_stats).map_err(mixed_source_error)?;
    let mut reader = SourceReader::new(source, inventory_limits).map_err(mixed_source_error)?;
    if u64::try_from(reader.length)
        .map_err(|_| mixed_source_error(ImmutableSourceError::Limit("length")))?
        != identity.length
    {
        return Err(mixed_source_error(ImmutableSourceError::Format(
            ImmutableError::Invalid("source length"),
        )));
    }
    let (minimum, maximum) = envelope.root.range.ok_or_else(|| {
        mixed_writer_error(ImmutableError::Invalid("persistent source root range"))
    })?;
    let root = PageRef {
        minimum,
        maximum,
        offset: u64::try_from(envelope.root.offset)
            .map_err(|_| mixed_source_error(ImmutableSourceError::Limit("offset")))?,
        level: envelope.root.level,
        digest: envelope.root.digest,
    };
    let level_count = usize::from(root.level)
        .checked_add(1)
        .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("page depth")))?;
    allocation_check::<Vec<OriginalMixedPage>>(level_count, limits.format)
        .map_err(mixed_writer_error)?;
    let mut originals = vec![Vec::new(); level_count];
    let mut locators = Vec::new();
    let mut visited = 0_usize;
    collect_source_mixed_inventory(
        &mut reader,
        &root,
        &mut originals,
        &mut locators,
        &mut visited,
    )?;
    if visited != strict.report.page_count
        || locators.len() != strict.report.object_count
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(mixed_writer_error(ImmutableError::Invalid(
            "persistent mixed source inventory",
        )));
    }
    let retained_metadata_allocation_bytes = mixed_metadata_allocation(&locators, &originals)?;
    if retained_metadata_allocation_bytes > limits.format.max_allocation_bytes {
        return Err(mixed_writer_error(ImmutableError::Limit("allocation")));
    }
    let order = source_mixed_operation_order(operations, &locators, limits.format)?;

    let base_len = reader.length;
    let mut tail = Vec::new();
    let locators = apply_source_mixed_operations(
        &mut tail,
        base_len,
        operations,
        &order,
        locators,
        limits.format,
    )?;
    let (next_root, pages_written, pages_reused) = materialize_persistent_tail_tree(
        &mut tail,
        base_len,
        &locators,
        &originals,
        limits.format,
    )
    .map_err(mixed_writer_error)?;
    let active_page_count = pages_written
        .checked_add(pages_reused)
        .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("page count")))?;
    let publication = PersistentTailPublication {
        base_len,
        sequence: strict
            .report
            .sequence
            .checked_add(1)
            .ok_or_else(|| mixed_writer_error(ImmutableError::Limit("sequence")))?,
        root: next_root,
        parent_snapshot_digest: strict.report.snapshot_digest,
        previous_footer_offset: u64::try_from(envelope.footer_offset)
            .map_err(|_| mixed_source_error(ImmutableSourceError::Limit("offset")))?,
        page_count: pages_written,
        object_count: locators.len(),
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits.format)
        .map_err(mixed_writer_error)?;
    report.page_count = active_page_count;
    persistent_tail_total_len(base_len, tail.len(), limits.format).map_err(mixed_writer_error)?;

    add_source_stats(&mut total_stats, reader.stats).map_err(mixed_source_error)?;
    Ok(PersistentSourceMixedInner {
        identity,
        tail,
        report,
        pages_written,
        pages_reused,
        source_stats: total_stats,
        retained_metadata_allocation_bytes,
    })
}

/// Plans one canonical deletion-plus-other-operation append tail from a versioned bounded source.
///
/// The planner retains decoded current locator/page metadata, never base bytes. Reuse requires exact
/// leaf or child-reference body equality with an authenticated original page at the same level.
pub fn plan_persistent_mixed_tail_at<S: PersistentVersionedReadAt>(
    source: &mut S,
    operations: &[ImmutableBatchOperation],
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceMixedPlan, PersistentSourceMixedError> {
    let version = source
        .version_token()
        .map_err(PersistentSourceMixedError::Version)?;
    let mut stable = PersistentReplacementStableSource::new(source, version);
    let result = plan_persistent_source_mixed_inner(&mut stable, operations, limits);
    if stable.changed {
        return Err(PersistentSourceMixedError::VersionChanged);
    }
    if let Some(error) = stable.version_error {
        return Err(PersistentSourceMixedError::Version(error));
    }
    let inner = result?;
    Ok(PersistentSourceMixedPlan {
        identity: inner.identity,
        version,
        tail_allocation_bytes: inner.tail.capacity(),
        tail: inner.tail,
        report: inner.report,
        pages_written: inner.pages_written,
        pages_reused: inner.pages_reused,
        source_stats: inner.source_stats,
        version_checks: stable.version_checks,
        retained_metadata_allocation_bytes: inner.retained_metadata_allocation_bytes,
    })
}

#[cfg(test)]
mod persistent_source_mixed_tests {
    use super::*;

    struct VersionedSlice {
        bytes: Vec<u8>,
        version: PersistentSourceVersion,
        reads: usize,
        mutate_after_read: Option<usize>,
    }

    impl ImmutableReadAt for VersionedSlice {
        fn len(&mut self) -> Result<u64, ImmutableSourceError> {
            u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
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
                self.bytes
                    .get(start..end)
                    .ok_or(ImmutableSourceError::Io("range"))?,
            );
            self.reads += 1;
            if self.mutate_after_read == Some(self.reads) {
                self.version.0[0] ^= 1;
            }
            Ok(())
        }
    }

    impl PersistentVersionedReadAt for VersionedSlice {
        fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
            Ok(self.version)
        }
    }

    fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::from(seed % 31 + 1),
            vec![seed; payload_len],
        )
    }

    fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
        (0..count)
            .map(|index| {
                object(
                    u64::try_from((index + 1) * 2).expect("id"),
                    u8::try_from(index % 251).expect("seed"),
                    17 + index % 29,
                )
            })
            .collect()
    }

    fn source_limits(format: ImmutableLimits, file_len: usize) -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            format,
            max_total_bytes_read: u64::try_from(file_len * 16).expect("budget"),
            max_read_operations: 2_000_000,
            max_read_request_bytes: 257,
            hash_block_bytes: 251,
        }
    }

    fn source(bytes: Vec<u8>, seed: u8) -> VersionedSlice {
        VersionedSlice {
            bytes,
            version: PersistentSourceVersion([seed; 32]),
            reads: 0,
            mutate_after_read: None,
        }
    }

    fn assert_matches_owned(
        base: &[u8],
        operations: &[ImmutableBatchOperation],
        format: ImmutableLimits,
    ) -> PersistentSourceMixedPlan {
        let owned = append_persistent_mixed_batch(base, operations, format).expect("owned mixed");
        let mut source = source(base.to_vec(), 101);
        let plan = plan_persistent_mixed_tail_at(
            &mut source,
            operations,
            source_limits(format, base.len()),
        )
        .expect("source mixed");
        assert_eq!(plan.tail, owned.bytes[base.len()..]);
        assert_eq!(plan.report, owned.report);
        assert_eq!(plan.pages_written, owned.pages_written);
        assert_eq!(plan.pages_reused, owned.pages_reused);
        assert_eq!(
            plan.identity,
            PersistentSourceIdentity::from_bytes(base).expect("identity")
        );
        assert!(plan.version_checks > 0);
        assert!(plan.tail_allocation_bytes < owned.bytes.len());
        assert!(plan.retained_metadata_allocation_bytes > 0);
        plan
    }

    #[test]
    fn stable_height_mixed_batch_matches_owned_and_is_order_independent() {
        let format = ImmutableLimits {
            max_file_bytes: 64 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
            max_allocation_bytes: 64 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = build_genesis(&even_objects(400), format).expect("base");
        let operations = vec![
            ImmutableBatchOperation::Delete(20),
            ImmutableBatchOperation::Put(object(200, 91, 73)),
            ImmutableBatchOperation::Put(object(741, 17, 41)),
        ];
        let first = assert_matches_owned(&base, &operations, format);
        let mut reversed = operations.clone();
        reversed.reverse();
        let second = assert_matches_owned(&base, &reversed, format);
        assert_eq!(first.tail, second.tail);
        assert_eq!(first.report, second.report);
    }

    #[test]
    fn root_collapse_and_growth_match_owned() {
        let format = ImmutableLimits {
            max_file_bytes: 64 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
            max_allocation_bytes: 64 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let collapse_base =
            build_genesis(&even_objects(2 * LEAF_MIN_OCCUPANCY), format).expect("collapse");
        let collapse = assert_matches_owned(
            &collapse_base,
            &[
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(4, 92, 19)),
            ],
            format,
        );
        assert_eq!(collapse.report.root_level, 0);

        let growth_base = build_genesis(&even_objects(LEAF_CAPACITY), format).expect("growth");
        let growth = assert_matches_owned(
            &growth_base,
            &[
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(1, 93, 21)),
                ImmutableBatchOperation::Put(object(3, 94, 23)),
            ],
            format,
        );
        assert_eq!(growth.report.root_level, 1);
    }

    #[test]
    fn invalid_batches_version_changes_and_budgets_are_rejected() {
        let format = ImmutableLimits::default();
        let base = build_genesis(&even_objects(32), format).expect("base");
        let operations = [
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(3, 95, 17)),
        ];
        let mut changed = source(base.clone(), 102);
        changed.mutate_after_read = Some(1);
        assert_eq!(
            plan_persistent_mixed_tail_at(
                &mut changed,
                &operations,
                source_limits(format, base.len()),
            )
            .expect_err("changed"),
            PersistentSourceMixedError::VersionChanged
        );

        let mut missing = source(base.clone(), 103);
        assert!(matches!(
            plan_persistent_mixed_tail_at(
                &mut missing,
                &[
                    ImmutableBatchOperation::Delete(999),
                    ImmutableBatchOperation::Put(object(3, 96, 19)),
                ],
                source_limits(format, base.len()),
            ),
            Err(PersistentSourceMixedError::Writer(ImmutableError::MissingObject(999)))
        ));

        let mut duplicate = source(base.clone(), 104);
        assert!(matches!(
            plan_persistent_mixed_tail_at(
                &mut duplicate,
                &[
                    ImmutableBatchOperation::Delete(2),
                    ImmutableBatchOperation::Put(object(2, 97, 21)),
                ],
                source_limits(format, base.len()),
            ),
            Err(PersistentSourceMixedError::Writer(ImmutableError::DuplicateObject(2)))
        ));

        let mut limited = source(base, 105);
        assert!(matches!(
            plan_persistent_mixed_tail_at(
                &mut limited,
                &operations,
                ImmutableSourceLimits {
                    format,
                    max_total_bytes_read: 1,
                    max_read_operations: 1,
                    max_read_request_bytes: 1,
                    hash_block_bytes: 1,
                },
            ),
            Err(PersistentSourceMixedError::Source(ImmutableSourceError::Limit(_)))
        ));
    }
}
