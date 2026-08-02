#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentSourceMultiPutError {
    Version(ImmutableSourceError),
    VersionChanged,
    Source(ImmutableSourceError),
    Writer(ImmutableError),
}

impl std::fmt::Display for PersistentSourceMultiPutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(error) => write!(formatter, "persistent source version failed: {error}"),
            Self::VersionChanged => write!(formatter, "persistent source version changed"),
            Self::Source(error) => write!(formatter, "persistent source planning failed: {error}"),
            Self::Writer(error) => write!(formatter, "persistent multi-Put tail failed: {error}"),
        }
    }
}

impl std::error::Error for PersistentSourceMultiPutError {}

impl From<ImmutableError> for PersistentSourceMultiPutError {
    fn from(error: ImmutableError) -> Self {
        Self::Writer(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentSourceMultiPutPlan {
    pub identity: PersistentSourceIdentity,
    pub version: PersistentSourceVersion,
    pub tail: Vec<u8>,
    pub report: ImmutableReport,
    pub pages_written: usize,
    pub pages_reused: usize,
    pub inserted_objects: usize,
    pub source_stats: ImmutableSourceStats,
    pub version_checks: u64,
    pub tail_allocation_bytes: usize,
}

struct PersistentSourceMultiPutInner {
    identity: PersistentSourceIdentity,
    tail: Vec<u8>,
    report: ImmutableReport,
    pages_written: usize,
    pages_reused: usize,
    inserted_objects: usize,
    source_stats: ImmutableSourceStats,
}

struct PersistentSourcePutRewrite {
    roots: Vec<PageRef>,
    inserted_objects: usize,
}

fn multi_put_source_error(error: ImmutableSourceError) -> PersistentSourceMultiPutError {
    PersistentSourceMultiPutError::Source(error)
}

fn multi_put_writer_error(error: ImmutableError) -> PersistentSourceMultiPutError {
    PersistentSourceMultiPutError::Writer(error)
}

#[allow(clippy::too_many_arguments)]
fn rewrite_source_put_paths<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    tail: &mut Vec<u8>,
    base_len: usize,
    reference: &PageRef,
    updates: &[Locator],
    pages_written: &mut usize,
    touched_original: &mut usize,
) -> Result<PersistentSourcePutRewrite, PersistentSourceMultiPutError> {
    if updates.is_empty() {
        return Ok(PersistentSourcePutRewrite {
            roots: vec![reference.clone()],
            inserted_objects: 0,
        });
    }
    increment_touched(touched_original, reader.limits.format).map_err(multi_put_writer_error)?;
    let node = read_source_deletion_node(reader, reference).map_err(|error| match error {
        PersistentSourceDeletionError::Source(error) => PersistentSourceMultiPutError::Source(error),
        PersistentSourceDeletionError::Writer(error) => PersistentSourceMultiPutError::Writer(error),
        PersistentSourceDeletionError::Version(error) => PersistentSourceMultiPutError::Version(error),
        PersistentSourceDeletionError::VersionChanged => PersistentSourceMultiPutError::VersionChanged,
    })?;

    match node {
        PendingDeletionNode::Leaf(existing) => {
            let inserted_objects = updates
                .iter()
                .filter(|update| {
                    existing
                        .binary_search_by_key(&update.object_id, |entry| entry.object_id)
                        .is_err()
                })
                .count();
            let merged = merge_leaf_puts(existing, updates, reader.limits.format)
                .map_err(multi_put_writer_error)?;
            let roots = append_leaf_tail_groups(
                tail,
                base_len,
                &merged,
                reader.limits.format,
                pages_written,
            )
            .map_err(multi_put_writer_error)?;
            Ok(PersistentSourcePutRewrite {
                roots,
                inserted_objects,
            })
        }
        PendingDeletionNode::Internal { level, children } => {
            let ranges = route_put_updates(&children, updates);
            let projected = children
                .len()
                .checked_add(updates.len())
                .ok_or_else(|| multi_put_writer_error(ImmutableError::Limit("page count")))?;
            allocation_check::<PageRef>(projected, reader.limits.format)
                .map_err(multi_put_writer_error)?;
            let mut rewritten = Vec::with_capacity(projected);
            let mut inserted_objects = 0_usize;
            for (index, child) in children.iter().enumerate() {
                let (start, end) = ranges[index];
                if start == end {
                    rewritten.push(child.clone());
                } else {
                    let child_rewrite = rewrite_source_put_paths(
                        reader,
                        tail,
                        base_len,
                        child,
                        &updates[start..end],
                        pages_written,
                        touched_original,
                    )?;
                    inserted_objects = inserted_objects
                        .checked_add(child_rewrite.inserted_objects)
                        .ok_or_else(|| {
                            multi_put_writer_error(ImmutableError::Limit("object count"))
                        })?;
                    rewritten.extend(child_rewrite.roots);
                }
            }
            let roots = append_internal_tail_groups(
                tail,
                base_len,
                &rewritten,
                level,
                reader.limits.format,
                pages_written,
            )
            .map_err(multi_put_writer_error)?;
            Ok(PersistentSourcePutRewrite {
                roots,
                inserted_objects,
            })
        }
    }
}

fn plan_persistent_source_multi_put_inner<S: ImmutableReadAt>(
    source: &mut S,
    inputs: &[ImmutableObjectInput],
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceMultiPutInner, PersistentSourceMultiPutError> {
    if inputs.is_empty() {
        return Err(multi_put_writer_error(ImmutableError::Invalid(
            "batch operations",
        )));
    }
    allocation_check::<usize>(inputs.len(), limits.format).map_err(multi_put_writer_error)?;
    let mut order: Vec<usize> = (0..inputs.len()).collect();
    order.sort_unstable_by_key(|index| inputs[*index].object_id);
    if let Some(pair) = order
        .windows(2)
        .find(|pair| inputs[pair[0]].object_id == inputs[pair[1]].object_id)
    {
        return Err(multi_put_writer_error(ImmutableError::DuplicateObject(
            inputs[pair[0]].object_id,
        )));
    }
    for index in &order {
        let input = &inputs[*index];
        if input.object_id == 0 || input.kind == 0 {
            return Err(multi_put_writer_error(ImmutableError::Invalid(
                "object input",
            )));
        }
    }

    let strict = validate_source_at(source, limits).map_err(multi_put_source_error)?;
    let mut total_stats = strict.stats;

    let canonical_limits =
        remaining_source_limits(limits, total_stats).map_err(multi_put_source_error)?;
    let (envelope, canonical_stats) =
        persistent_source_canonical_envelope(source, canonical_limits, &strict.report)
            .map_err(multi_put_source_error)?;
    add_source_stats(&mut total_stats, canonical_stats).map_err(multi_put_source_error)?;

    let identity_limits =
        remaining_source_limits(limits, total_stats).map_err(multi_put_source_error)?;
    let (identity, identity_stats) =
        persistent_source_identity(source, identity_limits).map_err(multi_put_source_error)?;
    add_source_stats(&mut total_stats, identity_stats).map_err(multi_put_source_error)?;

    let path_limits = remaining_source_limits(limits, total_stats).map_err(multi_put_source_error)?;
    let mut reader = SourceReader::new(source, path_limits).map_err(multi_put_source_error)?;
    if u64::try_from(reader.length)
        .map_err(|_| multi_put_source_error(ImmutableSourceError::Limit("length")))?
        != identity.length
    {
        return Err(multi_put_source_error(ImmutableSourceError::Format(
            ImmutableError::Invalid("source length"),
        )));
    }

    allocation_check::<Locator>(inputs.len(), limits.format).map_err(multi_put_writer_error)?;
    let base_len = reader.length;
    let mut tail = Vec::new();
    let mut updates = Vec::with_capacity(inputs.len());
    for index in order {
        updates.push(
            append_persistent_tail_object(&mut tail, base_len, &inputs[index], limits.format)
                .map_err(multi_put_writer_error)?,
        );
    }

    let (minimum, maximum) = envelope.root.range.ok_or_else(|| {
        multi_put_writer_error(ImmutableError::Invalid("persistent source root range"))
    })?;
    let root = PageRef {
        minimum,
        maximum,
        offset: u64::try_from(envelope.root.offset)
            .map_err(|_| multi_put_source_error(ImmutableSourceError::Limit("offset")))?,
        level: envelope.root.level,
        digest: envelope.root.digest,
    };
    let mut pages_written = 0_usize;
    let mut touched_original = 0_usize;
    let rewrite = rewrite_source_put_paths(
        &mut reader,
        &mut tail,
        base_len,
        &root,
        &updates,
        &mut pages_written,
        &mut touched_original,
    )?;
    let next_root = finish_put_tail_roots(
        &mut tail,
        base_len,
        rewrite.roots,
        root.level,
        limits.format,
        &mut pages_written,
    )
    .map_err(multi_put_writer_error)?;

    let next_object_count = strict
        .report
        .object_count
        .checked_add(rewrite.inserted_objects)
        .ok_or_else(|| multi_put_writer_error(ImmutableError::Limit("object count")))?;
    if next_object_count > limits.format.max_objects {
        return Err(multi_put_writer_error(ImmutableError::Limit("object count")));
    }
    let pages_reused = strict
        .report
        .page_count
        .checked_sub(touched_original)
        .ok_or_else(|| {
            multi_put_writer_error(ImmutableError::Invalid("persistent page accounting"))
        })?;
    let reachable_page_count = pages_reused
        .checked_add(pages_written)
        .ok_or_else(|| multi_put_writer_error(ImmutableError::Limit("page count")))?;
    let publication = PersistentTailPublication {
        base_len,
        sequence: strict
            .report
            .sequence
            .checked_add(1)
            .ok_or_else(|| multi_put_writer_error(ImmutableError::Limit("sequence")))?,
        root: next_root,
        parent_snapshot_digest: strict.report.snapshot_digest,
        previous_footer_offset: u64::try_from(envelope.footer_offset)
            .map_err(|_| multi_put_source_error(ImmutableSourceError::Limit("offset")))?,
        page_count: pages_written,
        object_count: next_object_count,
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits.format)
        .map_err(multi_put_writer_error)?;
    report.page_count = reachable_page_count;
    persistent_tail_total_len(base_len, tail.len(), limits.format).map_err(multi_put_writer_error)?;

    add_source_stats(&mut total_stats, reader.stats).map_err(multi_put_source_error)?;
    Ok(PersistentSourceMultiPutInner {
        identity,
        tail,
        report,
        pages_written,
        pages_reused,
        inserted_objects: rewrite.inserted_objects,
        source_stats: total_stats,
    })
}

/// Plans one shared persistent insertion/replacement append tail from a versioned bounded source.
///
/// All inputs are canonicalized by identifier. Affected leaves are merged once, shared ancestors are
/// rewritten once, untouched references remain exact, and all source phases share one budget/token.
pub fn plan_persistent_put_batch_tail_at<S: PersistentVersionedReadAt>(
    source: &mut S,
    inputs: &[ImmutableObjectInput],
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceMultiPutPlan, PersistentSourceMultiPutError> {
    let version = source
        .version_token()
        .map_err(PersistentSourceMultiPutError::Version)?;
    let mut stable = PersistentReplacementStableSource::new(source, version);
    let result = plan_persistent_source_multi_put_inner(&mut stable, inputs, limits);
    if stable.changed {
        return Err(PersistentSourceMultiPutError::VersionChanged);
    }
    if let Some(error) = stable.version_error {
        return Err(PersistentSourceMultiPutError::Version(error));
    }
    let inner = result?;
    Ok(PersistentSourceMultiPutPlan {
        identity: inner.identity,
        version,
        tail_allocation_bytes: inner.tail.capacity(),
        tail: inner.tail,
        report: inner.report,
        pages_written: inner.pages_written,
        pages_reused: inner.pages_reused,
        inserted_objects: inner.inserted_objects,
        source_stats: inner.source_stats,
        version_checks: stable.version_checks,
    })
}

#[cfg(test)]
mod persistent_source_multi_put_tests {
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

    fn base(count: usize, limits: ImmutableLimits) -> Vec<u8> {
        build_genesis(
            &(1..=count)
                .map(|index| {
                    object(
                        u64::try_from(index * 2).expect("id"),
                        u8::try_from(index % 251).expect("seed"),
                        1 + index % 23,
                    )
                })
                .collect::<Vec<_>>(),
            limits,
        )
        .expect("base")
    }

    fn source_limits(format: ImmutableLimits, file_len: usize) -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            format,
            max_total_bytes_read: u64::try_from(file_len * 14).expect("budget"),
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

    fn assert_matches_owned(base: &[u8], inputs: &[ImmutableObjectInput], format: ImmutableLimits) {
        let owned = append_persistent_put_batch(base, inputs, format).expect("owned multi-Put");
        let mut source = source(base.to_vec(), 81);
        let plan = plan_persistent_put_batch_tail_at(
            &mut source,
            inputs,
            source_limits(format, base.len()),
        )
        .expect("source multi-Put");
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
    }

    #[test]
    fn same_leaf_and_cross_leaf_batches_match_owned() {
        let format = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = base(400, format);
        assert_matches_owned(
            &base,
            &[object(3, 201, 7), object(5, 202, 9)],
            format,
        );
        assert_matches_owned(
            &base,
            &[object(1, 203, 11), object(799, 204, 13)],
            format,
        );
    }

    #[test]
    fn mixed_insertions_and_replacements_match_owned_and_are_order_independent() {
        let format = ImmutableLimits::default();
        let base = base(220, format);
        let inputs = vec![
            object(2, 205, 15),
            object(3, 206, 17),
            object(440, 207, 19),
            object(441, 208, 21),
        ];
        assert_matches_owned(&base, &inputs, format);
        let mut reversed = inputs.clone();
        reversed.reverse();
        let mut first_source = source(base.clone(), 82);
        let first = plan_persistent_put_batch_tail_at(
            &mut first_source,
            &inputs,
            source_limits(format, base.len()),
        )
        .expect("first");
        let mut second_source = source(base.clone(), 82);
        let second = plan_persistent_put_batch_tail_at(
            &mut second_source,
            &reversed,
            source_limits(format, base.len()),
        )
        .expect("second");
        assert_eq!(first.tail, second.tail);
        assert_eq!(first.report, second.report);
        assert_eq!(first.inserted_objects, 2);
    }

    #[test]
    fn simultaneous_splits_and_root_growth_match_owned() {
        let format = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = base(LEAF_CAPACITY, format);
        assert_matches_owned(
            &base,
            &[object(1, 209, 7), object(3, 210, 9), object(5, 211, 11)],
            format,
        );
    }

    #[test]
    fn duplicates_version_changes_and_budgets_are_rejected() {
        let format = ImmutableLimits::default();
        let base = base(32, format);
        let duplicate = [object(3, 212, 7), object(3, 213, 9)];
        let mut duplicate_source = source(base.clone(), 83);
        assert!(matches!(
            plan_persistent_put_batch_tail_at(
                &mut duplicate_source,
                &duplicate,
                source_limits(format, base.len()),
            ),
            Err(PersistentSourceMultiPutError::Writer(ImmutableError::DuplicateObject(3)))
        ));

        let inputs = [object(3, 214, 11), object(5, 215, 13)];
        let mut changed = source(base.clone(), 84);
        changed.mutate_after_read = Some(1);
        assert_eq!(
            plan_persistent_put_batch_tail_at(
                &mut changed,
                &inputs,
                source_limits(format, base.len()),
            )
            .expect_err("changed"),
            PersistentSourceMultiPutError::VersionChanged
        );

        let mut limited = source(base, 85);
        assert!(matches!(
            plan_persistent_put_batch_tail_at(
                &mut limited,
                &inputs,
                ImmutableSourceLimits {
                    format,
                    max_total_bytes_read: 1,
                    max_read_operations: 1,
                    max_read_request_bytes: 1,
                    hash_block_bytes: 1,
                },
            ),
            Err(PersistentSourceMultiPutError::Source(ImmutableSourceError::Limit(_)))
        ));
    }
}
