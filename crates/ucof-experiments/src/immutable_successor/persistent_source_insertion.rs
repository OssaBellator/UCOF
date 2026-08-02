#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentSourceInsertionError {
    Version(ImmutableSourceError),
    VersionChanged,
    Source(ImmutableSourceError),
    Writer(ImmutableError),
}

impl std::fmt::Display for PersistentSourceInsertionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(error) => write!(formatter, "persistent source version failed: {error}"),
            Self::VersionChanged => write!(formatter, "persistent source version changed"),
            Self::Source(error) => write!(formatter, "persistent source planning failed: {error}"),
            Self::Writer(error) => write!(formatter, "persistent insertion tail failed: {error}"),
        }
    }
}

impl std::error::Error for PersistentSourceInsertionError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentSourceInsertionPlan {
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

struct PersistentSourceInsertionInner {
    identity: PersistentSourceIdentity,
    tail: Vec<u8>,
    report: ImmutableReport,
    pages_written: usize,
    pages_reused: usize,
    source_stats: ImmutableSourceStats,
}

fn insertion_source_error(error: ImmutableSourceError) -> PersistentSourceInsertionError {
    PersistentSourceInsertionError::Source(error)
}

fn insertion_writer_error(error: ImmutableError) -> PersistentSourceInsertionError {
    PersistentSourceInsertionError::Writer(error)
}

fn read_persistent_insertion_page<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    reference: &LookupReference,
) -> Result<Vec<u8>, PersistentSourceInsertionError> {
    let page = reader
        .read_vec(reference.offset, PAGE_SIZE, "persistent insertion page")
        .map_err(insertion_source_error)?;
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(page.len())
                .map_err(|_| insertion_source_error(ImmutableSourceError::Limit("hashed bytes")))?,
        )
        .ok_or_else(|| insertion_source_error(ImmutableSourceError::Limit("hashed bytes")))?;
    if digest(&[PAGE_DOMAIN, &page]) != reference.digest || &page[..8] != PAGE_MAGIC {
        return Err(insertion_source_error(ImmutableSourceError::Format(
            ImmutableError::Invalid("persistent insertion page digest"),
        )));
    }
    let level = page[9];
    let minimum = u64_at(&page, 20, "persistent insertion page").map_err(insertion_writer_error)?;
    let maximum = u64_at(&page, 28, "persistent insertion page").map_err(insertion_writer_error)?;
    if level != reference.level
        || reference
            .range
            .is_some_and(|range| range != (minimum, maximum))
    {
        return Err(insertion_writer_error(ImmutableError::Invalid(
            "persistent insertion page reference",
        )));
    }
    Ok(page)
}

fn persistent_source_insert_path<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    reference: &LookupReference,
    inserted: &Locator,
    tail: &mut Vec<u8>,
    base_len: usize,
    pages_written: &mut usize,
) -> Result<Vec<PageRef>, PersistentSourceInsertionError> {
    let page = read_persistent_insertion_page(reader, reference)?;
    let kind = page[8];
    let level = page[9];
    let count = usize::try_from(
        u32_at(&page, 12, "persistent insertion page count").map_err(insertion_writer_error)?,
    )
    .map_err(|_| insertion_writer_error(ImmutableError::Invalid("persistent insertion page count")))?;
    let entry_size = usize::try_from(
        u32_at(&page, 16, "persistent insertion entry size").map_err(insertion_writer_error)?,
    )
    .map_err(|_| insertion_writer_error(ImmutableError::Invalid("persistent insertion entry size")))?;

    match kind {
        1 => {
            if level != 0 || count == 0 || count > LEAF_CAPACITY || entry_size != LEAF_ENTRY_LEN {
                return Err(insertion_writer_error(ImmutableError::Invalid(
                    "persistent insertion leaf",
                )));
            }
            allocation_check::<Locator>(
                count
                    .checked_add(1)
                    .ok_or(ImmutableError::Limit("object count"))?,
                reader.limits.format,
            )
            .map_err(insertion_writer_error)?;
            let mut entries = Vec::with_capacity(count + 1);
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                entries.push(Locator {
                    object_id: u64_at(&page, entry, "persistent insertion leaf")
                        .map_err(insertion_writer_error)?,
                    kind: u16_at(&page, entry + 8, "persistent insertion leaf")
                        .map_err(insertion_writer_error)?,
                    record_offset: u64_at(&page, entry + 16, "persistent insertion leaf")
                        .map_err(insertion_writer_error)?,
                    record_len: u64_at(&page, entry + 24, "persistent insertion leaf")
                        .map_err(insertion_writer_error)?,
                    logical_len: u64_at(&page, entry + 32, "persistent insertion leaf")
                        .map_err(insertion_writer_error)?,
                    digest: array(&page, entry + 40, "persistent insertion leaf")
                        .map_err(insertion_writer_error)?,
                });
            }
            let position = entries
                .binary_search_by_key(&inserted.object_id, |entry| entry.object_id)
                .unwrap_or_else(|position| position);
            if entries
                .get(position)
                .is_some_and(|entry| entry.object_id == inserted.object_id)
            {
                return Err(insertion_writer_error(ImmutableError::DuplicateObject(
                    inserted.object_id,
                )));
            }
            entries.insert(position, inserted.clone());
            if entries.len() <= LEAF_CAPACITY {
                return Ok(vec![append_persistent_tail_page(
                    tail,
                    base_len,
                    &encode_leaf(&entries).map_err(insertion_writer_error)?,
                    reader.limits.format,
                    pages_written,
                )
                .map_err(insertion_writer_error)?]);
            }
            let split = entries.len().div_ceil(2);
            let left = append_persistent_tail_page(
                tail,
                base_len,
                &encode_leaf(&entries[..split]).map_err(insertion_writer_error)?,
                reader.limits.format,
                pages_written,
            )
            .map_err(insertion_writer_error)?;
            let right = append_persistent_tail_page(
                tail,
                base_len,
                &encode_leaf(&entries[split..]).map_err(insertion_writer_error)?,
                reader.limits.format,
                pages_written,
            )
            .map_err(insertion_writer_error)?;
            Ok(vec![left, right])
        }
        2 => {
            if level == 0 || count == 0 || count > INTERNAL_FANOUT || entry_size != INTERNAL_ENTRY_LEN {
                return Err(insertion_writer_error(ImmutableError::Invalid(
                    "persistent insertion internal",
                )));
            }
            allocation_check::<PageRef>(
                count
                    .checked_add(1)
                    .ok_or(ImmutableError::Limit("page count"))?,
                reader.limits.format,
            )
            .map_err(insertion_writer_error)?;
            let mut children = Vec::with_capacity(count + 1);
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child_len = usize_at(&page, entry + 24, "persistent insertion child")
                    .map_err(insertion_writer_error)?;
                if child_len != PAGE_SIZE {
                    return Err(insertion_writer_error(ImmutableError::Invalid(
                        "persistent insertion child length",
                    )));
                }
                children.push(PageRef {
                    minimum: u64_at(&page, entry, "persistent insertion child")
                        .map_err(insertion_writer_error)?,
                    maximum: u64_at(&page, entry + 8, "persistent insertion child")
                        .map_err(insertion_writer_error)?,
                    offset: u64_at(&page, entry + 16, "persistent insertion child")
                        .map_err(insertion_writer_error)?,
                    level: level - 1,
                    digest: array(&page, entry + 32, "persistent insertion child")
                        .map_err(insertion_writer_error)?,
                });
            }
            let child_index = children
                .iter()
                .position(|child| inserted.object_id <= child.maximum)
                .unwrap_or(children.len() - 1);
            let child = &children[child_index];
            let child_reference = LookupReference {
                offset: usize::try_from(child.offset)
                    .map_err(|_| insertion_source_error(ImmutableSourceError::Limit("offset")))?,
                level: child.level,
                digest: child.digest,
                range: Some((child.minimum, child.maximum)),
            };
            let replacements = persistent_source_insert_path(
                reader,
                &child_reference,
                inserted,
                tail,
                base_len,
                pages_written,
            )?;
            let updated_len = children
                .len()
                .checked_sub(1)
                .and_then(|value| value.checked_add(replacements.len()))
                .ok_or_else(|| insertion_writer_error(ImmutableError::Limit("page count")))?;
            allocation_check::<PageRef>(updated_len, reader.limits.format)
                .map_err(insertion_writer_error)?;
            let mut updated = Vec::with_capacity(updated_len);
            updated.extend_from_slice(&children[..child_index]);
            updated.extend(replacements);
            updated.extend_from_slice(&children[child_index + 1..]);
            if updated.len() <= INTERNAL_FANOUT {
                return Ok(vec![append_persistent_tail_page(
                    tail,
                    base_len,
                    &encode_internal(&updated, level).map_err(insertion_writer_error)?,
                    reader.limits.format,
                    pages_written,
                )
                .map_err(insertion_writer_error)?]);
            }
            let split = updated.len().div_ceil(2);
            let left = append_persistent_tail_page(
                tail,
                base_len,
                &encode_internal(&updated[..split], level).map_err(insertion_writer_error)?,
                reader.limits.format,
                pages_written,
            )
            .map_err(insertion_writer_error)?;
            let right = append_persistent_tail_page(
                tail,
                base_len,
                &encode_internal(&updated[split..], level).map_err(insertion_writer_error)?,
                reader.limits.format,
                pages_written,
            )
            .map_err(insertion_writer_error)?;
            Ok(vec![left, right])
        }
        _ => Err(insertion_writer_error(ImmutableError::Invalid(
            "persistent insertion page kind",
        ))),
    }
}

fn plan_persistent_source_insertion_inner<S: ImmutableReadAt>(
    source: &mut S,
    input: &ImmutableObjectInput,
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceInsertionInner, PersistentSourceInsertionError> {
    if input.object_id == 0 || input.kind == 0 {
        return Err(insertion_writer_error(ImmutableError::Invalid(
            "object identity",
        )));
    }

    let strict = validate_source_at(source, limits).map_err(insertion_source_error)?;
    if strict.report.object_count >= limits.format.max_objects {
        return Err(insertion_writer_error(ImmutableError::Limit("object count")));
    }
    let mut total_stats = strict.stats;

    let canonical_limits = remaining_source_limits(limits, total_stats).map_err(insertion_source_error)?;
    let (envelope, canonical_stats) =
        persistent_source_canonical_envelope(source, canonical_limits, &strict.report)
            .map_err(insertion_source_error)?;
    add_source_stats(&mut total_stats, canonical_stats).map_err(insertion_source_error)?;

    let identity_limits = remaining_source_limits(limits, total_stats).map_err(insertion_source_error)?;
    let (identity, identity_stats) =
        persistent_source_identity(source, identity_limits).map_err(insertion_source_error)?;
    add_source_stats(&mut total_stats, identity_stats).map_err(insertion_source_error)?;

    let path_limits = remaining_source_limits(limits, total_stats).map_err(insertion_source_error)?;
    let mut reader = SourceReader::new(source, path_limits).map_err(insertion_source_error)?;
    if u64::try_from(reader.length)
        .map_err(|_| insertion_source_error(ImmutableSourceError::Limit("length")))?
        != identity.length
    {
        return Err(insertion_source_error(ImmutableSourceError::Format(
            ImmutableError::Invalid("source length"),
        )));
    }

    let base_len = reader.length;
    let touched_pages = usize::from(envelope.root.level)
        .checked_add(1)
        .ok_or_else(|| insertion_writer_error(ImmutableError::Limit("page depth")))?;
    let pages_reused = strict
        .report
        .page_count
        .checked_sub(touched_pages)
        .ok_or_else(|| insertion_writer_error(ImmutableError::Invalid("persistent page accounting")))?;

    let mut tail = Vec::new();
    let inserted = append_persistent_tail_object(&mut tail, base_len, input, limits.format)
        .map_err(insertion_writer_error)?;
    let mut pages_written = 0_usize;
    let mut roots = persistent_source_insert_path(
        &mut reader,
        &envelope.root,
        &inserted,
        &mut tail,
        base_len,
        &mut pages_written,
    )?;
    let next_root = match roots.len() {
        1 => roots
            .pop()
            .ok_or_else(|| insertion_writer_error(ImmutableError::Invalid("persistent insertion root")))?,
        2 => {
            let next_level = envelope
                .root
                .level
                .checked_add(1)
                .ok_or_else(|| insertion_writer_error(ImmutableError::Limit("page depth")))?;
            if next_level > limits.format.max_depth {
                return Err(insertion_writer_error(ImmutableError::Limit("page depth")));
            }
            append_persistent_tail_page(
                &mut tail,
                base_len,
                &encode_internal(&roots, next_level).map_err(insertion_writer_error)?,
                limits.format,
                &mut pages_written,
            )
            .map_err(insertion_writer_error)?
        }
        _ => {
            return Err(insertion_writer_error(ImmutableError::Invalid(
                "persistent insertion root",
            )))
        }
    };

    let object_count = strict
        .report
        .object_count
        .checked_add(1)
        .ok_or_else(|| insertion_writer_error(ImmutableError::Limit("object count")))?;
    let reachable_page_count = pages_reused
        .checked_add(pages_written)
        .ok_or_else(|| insertion_writer_error(ImmutableError::Limit("page count")))?;
    let publication = PersistentTailPublication {
        base_len,
        sequence: strict
            .report
            .sequence
            .checked_add(1)
            .ok_or_else(|| insertion_writer_error(ImmutableError::Limit("sequence")))?,
        root: next_root,
        parent_snapshot_digest: strict.report.snapshot_digest,
        previous_footer_offset: u64::try_from(envelope.footer_offset)
            .map_err(|_| insertion_source_error(ImmutableSourceError::Limit("offset")))?,
        page_count: pages_written,
        object_count,
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits.format)
        .map_err(insertion_writer_error)?;
    report.page_count = reachable_page_count;
    persistent_tail_total_len(base_len, tail.len(), limits.format).map_err(insertion_writer_error)?;

    add_source_stats(&mut total_stats, reader.stats).map_err(insertion_source_error)?;
    Ok(PersistentSourceInsertionInner {
        identity,
        tail,
        report,
        pages_written,
        pages_reused,
        source_stats: total_stats,
    })
}

/// Plans one persistent insertion append tail directly from a strongly versioned bounded source.
///
/// The source is strictly validated, independently checked for canonical occupancy, and hashed for
/// a whole-file identity before the insertion path is reread. Only the append tail is retained.
pub fn plan_persistent_insertion_tail_at<S: PersistentVersionedReadAt>(
    source: &mut S,
    input: &ImmutableObjectInput,
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceInsertionPlan, PersistentSourceInsertionError> {
    let version = source
        .version_token()
        .map_err(PersistentSourceInsertionError::Version)?;
    let mut stable = PersistentReplacementStableSource::new(source, version);
    let result = plan_persistent_source_insertion_inner(&mut stable, input, limits);
    if stable.changed {
        return Err(PersistentSourceInsertionError::VersionChanged);
    }
    if let Some(error) = stable.version_error {
        return Err(PersistentSourceInsertionError::Version(error));
    }
    let inner = result?;
    Ok(PersistentSourceInsertionPlan {
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
mod persistent_source_insertion_tests {
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

    #[test]
    fn source_planned_insertion_matches_owned_writer() {
        let format = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = base(220, format);
        let input = object(3, 211, 17);
        let operations = [ImmutableBatchOperation::Put(input.clone())];
        let owned = append_persistent_batch(&base, &operations, format).expect("owned");
        let mut source = source(base.clone(), 41);
        let plan = plan_persistent_insertion_tail_at(
            &mut source,
            &input,
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
    fn full_root_leaf_split_matches_owned_root_growth() {
        let format = ImmutableLimits::default();
        let base = base(LEAF_CAPACITY, format);
        let input = object(1, 212, 19);
        let operations = [ImmutableBatchOperation::Put(input.clone())];
        let owned = append_persistent_batch(&base, &operations, format).expect("owned");
        let mut source = source(base.clone(), 42);
        let plan = plan_persistent_insertion_tail_at(
            &mut source,
            &input,
            source_limits(format, base.len()),
        )
        .expect("source plan");
        assert_eq!(plan.tail, owned.bytes[base.len()..]);
        assert_eq!(plan.report, owned.report);
        assert_eq!(plan.pages_written, owned.pages_written);
        assert_eq!(plan.pages_reused, owned.pages_reused);
        assert_eq!(plan.report.root_level, 1);
    }

    #[test]
    fn duplicate_identifier_is_rejected() {
        let format = ImmutableLimits::default();
        let base = base(32, format);
        let mut source = source(base.clone(), 43);
        let error = plan_persistent_insertion_tail_at(
            &mut source,
            &object(2, 213, 7),
            source_limits(format, base.len()),
        )
        .expect_err("duplicate");
        assert_eq!(
            error,
            PersistentSourceInsertionError::Writer(ImmutableError::DuplicateObject(2))
        );
    }

    #[test]
    fn source_version_change_rejects_without_a_plan() {
        let format = ImmutableLimits::default();
        let base = base(16, format);
        let mut source = source(base.clone(), 44);
        source.mutate_after_read = Some(1);
        let error = plan_persistent_insertion_tail_at(
            &mut source,
            &object(3, 214, 9),
            source_limits(format, base.len()),
        )
        .expect_err("changed version");
        assert_eq!(error, PersistentSourceInsertionError::VersionChanged);
    }

    #[test]
    fn cumulative_budget_exhaustion_is_reported() {
        let format = ImmutableLimits::default();
        let base = base(16, format);
        let mut source = source(base, 45);
        let error = plan_persistent_insertion_tail_at(
            &mut source,
            &object(3, 215, 9),
            ImmutableSourceLimits {
                format,
                max_total_bytes_read: 1,
                max_read_operations: 1,
                max_read_request_bytes: 1,
                hash_block_bytes: 1,
            },
        )
        .expect_err("budget");
        assert!(matches!(
            error,
            PersistentSourceInsertionError::Source(ImmutableSourceError::Limit(_))
        ));
    }
}
