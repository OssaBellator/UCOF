#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentSourceDeletionError {
    Version(ImmutableSourceError),
    VersionChanged,
    Source(ImmutableSourceError),
    Writer(ImmutableError),
}

impl std::fmt::Display for PersistentSourceDeletionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(error) => write!(formatter, "persistent source version failed: {error}"),
            Self::VersionChanged => write!(formatter, "persistent source version changed"),
            Self::Source(error) => write!(formatter, "persistent source planning failed: {error}"),
            Self::Writer(error) => write!(formatter, "persistent deletion tail failed: {error}"),
        }
    }
}

impl std::error::Error for PersistentSourceDeletionError {}

impl From<ImmutableError> for PersistentSourceDeletionError {
    fn from(error: ImmutableError) -> Self {
        Self::Writer(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentSourceDeletionPlan {
    pub identity: PersistentSourceIdentity,
    pub version: PersistentSourceVersion,
    pub tail: Vec<u8>,
    pub report: ImmutableReport,
    pub pages_written: usize,
    pub pages_reused: usize,
    pub source_stats: ImmutableSourceStats,
    pub version_checks: u64,
    pub tail_allocation_bytes: usize,
}

struct PersistentSourceDeletionInner {
    identity: PersistentSourceIdentity,
    tail: Vec<u8>,
    report: ImmutableReport,
    pages_written: usize,
    pages_reused: usize,
    source_stats: ImmutableSourceStats,
}

fn deletion_source_error(error: ImmutableSourceError) -> PersistentSourceDeletionError {
    PersistentSourceDeletionError::Source(error)
}

fn deletion_writer_error(error: ImmutableError) -> PersistentSourceDeletionError {
    PersistentSourceDeletionError::Writer(error)
}

fn source_deletion_reference(reference: &PageRef) -> Result<LookupReference, PersistentSourceDeletionError> {
    Ok(LookupReference {
        offset: usize::try_from(reference.offset)
            .map_err(|_| deletion_source_error(ImmutableSourceError::Limit("offset")))?,
        level: reference.level,
        digest: reference.digest,
        range: Some((reference.minimum, reference.maximum)),
    })
}

fn read_source_deletion_node<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    reference: &PageRef,
) -> Result<PendingDeletionNode, PersistentSourceDeletionError> {
    let lookup = source_deletion_reference(reference)?;
    let page = reader
        .read_vec(lookup.offset, PAGE_SIZE, "persistent deletion page")
        .map_err(deletion_source_error)?;
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(page.len())
                .map_err(|_| deletion_source_error(ImmutableSourceError::Limit("hashed bytes")))?,
        )
        .ok_or_else(|| deletion_source_error(ImmutableSourceError::Limit("hashed bytes")))?;
    if digest(&[PAGE_DOMAIN, &page]) != lookup.digest || &page[..8] != PAGE_MAGIC {
        return Err(deletion_source_error(ImmutableSourceError::Format(
            ImmutableError::Invalid("persistent deletion page digest"),
        )));
    }
    let minimum = u64_at(&page, 20, "persistent deletion page").map_err(deletion_writer_error)?;
    let maximum = u64_at(&page, 28, "persistent deletion page").map_err(deletion_writer_error)?;
    if page[9] != lookup.level || lookup.range != Some((minimum, maximum)) {
        return Err(deletion_writer_error(ImmutableError::Invalid(
            "persistent deletion page reference",
        )));
    }
    if reference.level == 0 {
        Ok(PendingDeletionNode::Leaf(
            decode_persistent_leaf(&page, reference, reader.limits.format)
                .map_err(deletion_writer_error)?,
        ))
    } else {
        Ok(PendingDeletionNode::Internal {
            level: reference.level,
            children: decode_persistent_children(&page, reference, reader.limits.format)
                .map_err(deletion_writer_error)?,
        })
    }
}

fn materialize_source_deletion_node(
    tail: &mut Vec<u8>,
    base_len: usize,
    node: &PendingDeletionNode,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<PageRef, PersistentSourceDeletionError> {
    let page = match node {
        PendingDeletionNode::Leaf(entries) => encode_leaf(entries),
        PendingDeletionNode::Internal { level, children } => encode_internal(children, *level),
    }
    .map_err(deletion_writer_error)?;
    append_persistent_tail_page(tail, base_len, &page, limits, pages_written)
        .map_err(deletion_writer_error)
}

#[allow(clippy::too_many_arguments)]
fn delete_source_persistent_node<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    tail: &mut Vec<u8>,
    base_len: usize,
    reference: &PageRef,
    object_id: u64,
    pages_written: &mut usize,
    touched_original: &mut usize,
) -> Result<PendingDeletionNode, PersistentSourceDeletionError> {
    increment_touched(touched_original, reader.limits.format).map_err(deletion_writer_error)?;
    let mut node = read_source_deletion_node(reader, reference)?;
    if let PendingDeletionNode::Leaf(entries) = &mut node {
        let position = entries
            .binary_search_by_key(&object_id, |entry| entry.object_id)
            .map_err(|_| deletion_writer_error(ImmutableError::MissingObject(object_id)))?;
        entries.remove(position);
        return Ok(node);
    }

    let PendingDeletionNode::Internal { level, children } = &mut node else {
        return Err(deletion_writer_error(ImmutableError::Invalid(
            "deletion node",
        )));
    };
    let child_index = children
        .iter()
        .position(|child| child.minimum <= object_id && object_id <= child.maximum)
        .ok_or_else(|| deletion_writer_error(ImmutableError::MissingObject(object_id)))?;
    let mut target = delete_source_persistent_node(
        reader,
        tail,
        base_len,
        &children[child_index],
        object_id,
        pages_written,
        touched_original,
    )?;
    let minimum = deletion_minimum(target.level());

    if target.occupancy() >= minimum {
        children[child_index] = materialize_source_deletion_node(
            tail,
            base_len,
            &target,
            reader.limits.format,
            pages_written,
        )?;
        return Ok(node);
    }

    let mut left = if child_index > 0 {
        Some(read_source_deletion_node(reader, &children[child_index - 1])?)
    } else {
        None
    };
    if let Some(left_node) = &mut left {
        if left_node.occupancy() > minimum {
            increment_touched(touched_original, reader.limits.format)
                .map_err(deletion_writer_error)?;
            borrow_deletion_from_left(left_node, &mut target, reader.limits.format)
                .map_err(deletion_writer_error)?;
            children[child_index - 1] = materialize_source_deletion_node(
                tail,
                base_len,
                left_node,
                reader.limits.format,
                pages_written,
            )?;
            children[child_index] = materialize_source_deletion_node(
                tail,
                base_len,
                &target,
                reader.limits.format,
                pages_written,
            )?;
            return Ok(node);
        }
    }

    let mut right = if child_index + 1 < children.len() {
        Some(read_source_deletion_node(reader, &children[child_index + 1])?)
    } else {
        None
    };
    if let Some(right_node) = &mut right {
        if right_node.occupancy() > minimum {
            increment_touched(touched_original, reader.limits.format)
                .map_err(deletion_writer_error)?;
            borrow_deletion_from_right(&mut target, right_node, reader.limits.format)
                .map_err(deletion_writer_error)?;
            children[child_index] = materialize_source_deletion_node(
                tail,
                base_len,
                &target,
                reader.limits.format,
                pages_written,
            )?;
            children[child_index + 1] = materialize_source_deletion_node(
                tail,
                base_len,
                right_node,
                reader.limits.format,
                pages_written,
            )?;
            return Ok(node);
        }
    }

    if let Some(left_node) = left {
        increment_touched(touched_original, reader.limits.format).map_err(deletion_writer_error)?;
        let merged = merge_deletion_nodes(left_node, target, reader.limits.format)
            .map_err(deletion_writer_error)?;
        children[child_index - 1] = materialize_source_deletion_node(
            tail,
            base_len,
            &merged,
            reader.limits.format,
            pages_written,
        )?;
        children.remove(child_index);
    } else if let Some(right_node) = right {
        increment_touched(touched_original, reader.limits.format).map_err(deletion_writer_error)?;
        let merged = merge_deletion_nodes(target, right_node, reader.limits.format)
            .map_err(deletion_writer_error)?;
        children[child_index] = materialize_source_deletion_node(
            tail,
            base_len,
            &merged,
            reader.limits.format,
            pages_written,
        )?;
        children.remove(child_index + 1);
    } else {
        return Err(deletion_writer_error(ImmutableError::Invalid(
            "deletion sibling",
        )));
    }

    if *level == 0 {
        return Err(deletion_writer_error(ImmutableError::Invalid(
            "deletion internal level",
        )));
    }
    Ok(node)
}

fn plan_persistent_source_deletion_inner<S: ImmutableReadAt>(
    source: &mut S,
    object_id: u64,
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceDeletionInner, PersistentSourceDeletionError> {
    if object_id == 0 {
        return Err(deletion_writer_error(ImmutableError::Invalid(
            "batch object id",
        )));
    }

    let strict = validate_source_at(source, limits).map_err(deletion_source_error)?;
    if strict.report.object_count <= 1 {
        return Err(deletion_writer_error(ImmutableError::Invalid(
            "batch result",
        )));
    }
    let mut total_stats = strict.stats;

    let canonical_limits =
        remaining_source_limits(limits, total_stats).map_err(deletion_source_error)?;
    let (envelope, canonical_stats) =
        persistent_source_canonical_envelope(source, canonical_limits, &strict.report)
            .map_err(deletion_source_error)?;
    add_source_stats(&mut total_stats, canonical_stats).map_err(deletion_source_error)?;

    let identity_limits =
        remaining_source_limits(limits, total_stats).map_err(deletion_source_error)?;
    let (identity, identity_stats) =
        persistent_source_identity(source, identity_limits).map_err(deletion_source_error)?;
    add_source_stats(&mut total_stats, identity_stats).map_err(deletion_source_error)?;

    let path_limits = remaining_source_limits(limits, total_stats).map_err(deletion_source_error)?;
    let mut reader = SourceReader::new(source, path_limits).map_err(deletion_source_error)?;
    if u64::try_from(reader.length)
        .map_err(|_| deletion_source_error(ImmutableSourceError::Limit("length")))?
        != identity.length
    {
        return Err(deletion_source_error(ImmutableSourceError::Format(
            ImmutableError::Invalid("source length"),
        )));
    }

    let root = PageRef {
        minimum: envelope.root.range.map_or(0, |range| range.0),
        maximum: envelope.root.range.map_or(0, |range| range.1),
        offset: u64::try_from(envelope.root.offset)
            .map_err(|_| deletion_source_error(ImmutableSourceError::Limit("offset")))?,
        level: envelope.root.level,
        digest: envelope.root.digest,
    };
    let base_len = reader.length;
    let mut tail = Vec::new();
    let mut pages_written = 0_usize;
    let mut touched_original = 0_usize;
    let pending = delete_source_persistent_node(
        &mut reader,
        &mut tail,
        base_len,
        &root,
        object_id,
        &mut pages_written,
        &mut touched_original,
    )?;

    let next_root = match pending {
        PendingDeletionNode::Leaf(entries) => materialize_source_deletion_node(
            &mut tail,
            base_len,
            &PendingDeletionNode::Leaf(entries),
            limits.format,
            &mut pages_written,
        )?,
        PendingDeletionNode::Internal { level: _, children } if children.len() == 1 => children
            .into_iter()
            .next()
            .ok_or_else(|| deletion_writer_error(ImmutableError::Invalid("deletion root collapse")))?,
        PendingDeletionNode::Internal { level, children } => materialize_source_deletion_node(
            &mut tail,
            base_len,
            &PendingDeletionNode::Internal { level, children },
            limits.format,
            &mut pages_written,
        )?,
    };

    let pages_reused = strict
        .report
        .page_count
        .checked_sub(touched_original)
        .ok_or_else(|| deletion_writer_error(ImmutableError::Invalid("persistent page accounting")))?;
    let reachable_page_count = pages_reused
        .checked_add(pages_written)
        .ok_or_else(|| deletion_writer_error(ImmutableError::Limit("page count")))?;
    let object_count = strict
        .report
        .object_count
        .checked_sub(1)
        .ok_or_else(|| deletion_writer_error(ImmutableError::Invalid("batch result")))?;
    let publication = PersistentTailPublication {
        base_len,
        sequence: strict
            .report
            .sequence
            .checked_add(1)
            .ok_or_else(|| deletion_writer_error(ImmutableError::Limit("sequence")))?,
        root: next_root,
        parent_snapshot_digest: strict.report.snapshot_digest,
        previous_footer_offset: u64::try_from(envelope.footer_offset)
            .map_err(|_| deletion_source_error(ImmutableSourceError::Limit("offset")))?,
        page_count: pages_written,
        object_count,
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits.format)
        .map_err(deletion_writer_error)?;
    report.page_count = reachable_page_count;
    persistent_tail_total_len(base_len, tail.len(), limits.format).map_err(deletion_writer_error)?;

    add_source_stats(&mut total_stats, reader.stats).map_err(deletion_source_error)?;
    Ok(PersistentSourceDeletionInner {
        identity,
        tail,
        report,
        pages_written,
        pages_reused,
        source_stats: total_stats,
    })
}

/// Plans one persistent deletion append tail directly from a strongly versioned bounded source.
///
/// Target and sibling pages are authenticated before deterministic borrow/merge repair. The source
/// is strictly validated, checked for canonical occupancy, and hashed for a complete base identity.
/// Only the append tail is retained.
pub fn plan_persistent_deletion_tail_at<S: PersistentVersionedReadAt>(
    source: &mut S,
    object_id: u64,
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceDeletionPlan, PersistentSourceDeletionError> {
    let version = source
        .version_token()
        .map_err(PersistentSourceDeletionError::Version)?;
    let mut stable = PersistentReplacementStableSource::new(source, version);
    let result = plan_persistent_source_deletion_inner(&mut stable, object_id, limits);
    if stable.changed {
        return Err(PersistentSourceDeletionError::VersionChanged);
    }
    if let Some(error) = stable.version_error {
        return Err(PersistentSourceDeletionError::Version(error));
    }
    let inner = result?;
    Ok(PersistentSourceDeletionPlan {
        identity: inner.identity,
        version,
        tail_allocation_bytes: inner.tail.capacity(),
        tail: inner.tail,
        report: inner.report,
        pages_written: inner.pages_written,
        pages_reused: inner.pages_reused,
        source_stats: inner.source_stats,
        version_checks: stable.version_checks,
    })
}

#[cfg(test)]
mod persistent_source_deletion_tests {
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

    fn objects(count: usize) -> Vec<ImmutableObjectInput> {
        (1..=u64::try_from(count).expect("count"))
            .map(|object_id| ImmutableObjectInput::new(object_id, 1, vec![object_id as u8]))
            .collect()
    }

    fn source_limits(format: ImmutableLimits, file_len: usize) -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            format,
            max_total_bytes_read: u64::try_from(file_len * 12).expect("budget"),
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

    fn assert_matches_owned(count: usize, object_id: u64, seed: u8) {
        let format = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = build_genesis(&objects(count), format).expect("base");
        let owned = append_persistent_delete(&base, object_id, format).expect("owned");
        let mut source = source(base.clone(), seed);
        let plan = plan_persistent_deletion_tail_at(
            &mut source,
            object_id,
            source_limits(format, base.len()),
        )
        .expect("source plan");
        assert_eq!(plan.tail, owned.bytes[base.len()..]);
        assert_eq!(plan.report, owned.report);
        assert_eq!(plan.pages_written, owned.pages_written);
        assert_eq!(plan.pages_reused, owned.pages_reused);
        assert_eq!(
            plan.identity,
            PersistentSourceIdentity::from_bytes(&base).expect("identity")
        );
        assert!(plan.version_checks > 0);
        assert!(plan.tail_allocation_bytes < owned.bytes.len());
    }

    #[test]
    fn root_leaf_and_no_underflow_match_owned() {
        assert_matches_owned(10, 5, 61);
        assert_matches_owned(400, 10, 62);
    }

    #[test]
    fn left_borrow_matches_owned() {
        let count = LEAF_CAPACITY + 2;
        assert_matches_owned(count, u64::try_from(count).expect("count"), 63);
    }

    #[test]
    fn merge_and_root_collapse_match_owned() {
        assert_matches_owned(2 * LEAF_MIN_OCCUPANCY, 1, 64);
    }

    #[test]
    fn missing_and_final_object_deletions_are_rejected() {
        let format = ImmutableLimits::default();
        let base = build_genesis(&objects(8), format).expect("base");
        let mut missing = source(base.clone(), 65);
        assert_eq!(
            plan_persistent_deletion_tail_at(
                &mut missing,
                99,
                source_limits(format, base.len()),
            )
            .expect_err("missing"),
            PersistentSourceDeletionError::Writer(ImmutableError::MissingObject(99))
        );
        let one = build_genesis(&objects(1), format).expect("one");
        let mut final_source = source(one.clone(), 66);
        assert!(matches!(
            plan_persistent_deletion_tail_at(
                &mut final_source,
                1,
                source_limits(format, one.len()),
            ),
            Err(PersistentSourceDeletionError::Writer(ImmutableError::Invalid(_)))
        ));
    }

    #[test]
    fn version_change_and_budget_exhaustion_are_reported() {
        let format = ImmutableLimits::default();
        let base = build_genesis(&objects(16), format).expect("base");
        let mut changed = source(base.clone(), 67);
        changed.mutate_after_read = Some(1);
        assert_eq!(
            plan_persistent_deletion_tail_at(
                &mut changed,
                2,
                source_limits(format, base.len()),
            )
            .expect_err("version change"),
            PersistentSourceDeletionError::VersionChanged
        );
        let mut limited = source(base, 68);
        assert!(matches!(
            plan_persistent_deletion_tail_at(
                &mut limited,
                2,
                ImmutableSourceLimits {
                    format,
                    max_total_bytes_read: 1,
                    max_read_operations: 1,
                    max_read_request_bytes: 1,
                    hash_block_bytes: 1,
                },
            ),
            Err(PersistentSourceDeletionError::Source(ImmutableSourceError::Limit(_)))
        ));
    }
}
