#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentSourceReplacementError {
    Version(ImmutableSourceError),
    VersionChanged,
    Source(ImmutableSourceError),
    Writer(ImmutableError),
}

impl std::fmt::Display for PersistentSourceReplacementError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(error) => write!(formatter, "persistent source version failed: {error}"),
            Self::VersionChanged => write!(formatter, "persistent source version changed"),
            Self::Source(error) => write!(formatter, "persistent source planning failed: {error}"),
            Self::Writer(error) => write!(formatter, "persistent tail construction failed: {error}"),
        }
    }
}

impl std::error::Error for PersistentSourceReplacementError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentSourceReplacementPlan {
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

struct PersistentReplacementStableSource<'a, S> {
    inner: &'a mut S,
    expected: PersistentSourceVersion,
    version_checks: u64,
    changed: bool,
    version_error: Option<ImmutableSourceError>,
}

impl<'a, S: PersistentVersionedReadAt> PersistentReplacementStableSource<'a, S> {
    fn new(inner: &'a mut S, expected: PersistentSourceVersion) -> Self {
        Self {
            inner,
            expected,
            version_checks: 0,
            changed: false,
            version_error: None,
        }
    }

    fn ensure_stable(&mut self) -> Result<(), ImmutableSourceError> {
        self.version_checks = self
            .version_checks
            .checked_add(1)
            .ok_or(ImmutableSourceError::Limit("version checks"))?;
        match self.inner.version_token() {
            Ok(actual) if actual == self.expected => Ok(()),
            Ok(_) => {
                self.changed = true;
                Err(ImmutableSourceError::Io("version changed"))
            }
            Err(error) => {
                self.version_error = Some(error.clone());
                Err(error)
            }
        }
    }
}

impl<S: PersistentVersionedReadAt> ImmutableReadAt for PersistentReplacementStableSource<'_, S> {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        self.ensure_stable()?;
        let length = self.inner.len()?;
        self.ensure_stable()?;
        Ok(length)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        self.ensure_stable()?;
        self.inner.read_exact_at(offset, buffer)?;
        self.ensure_stable()
    }
}

fn persistent_source_identity<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<(PersistentSourceIdentity, ImmutableSourceStats), ImmutableSourceError> {
    let mut reader = SourceReader::new(source, limits)?;
    let length = reader.length;
    let mut hasher = Sha256::new();
    reader.hash_range(&mut hasher, 0, length, "source identity")?;
    Ok((
        PersistentSourceIdentity {
            length: u64::try_from(length).map_err(|_| ImmutableSourceError::Limit("length"))?,
            sha256: hasher.finalize().into(),
        },
        reader.stats,
    ))
}

fn persistent_source_replacement_page<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    envelope: &LookupEnvelope,
    reference: &LookupReference,
    replacements: &std::collections::BTreeMap<u64, Locator>,
    tail: &mut Vec<u8>,
    base_len: usize,
    pages_written: &mut usize,
) -> Result<(PageRef, bool), PersistentSourceReplacementError> {
    let page = reader
        .read_vec(reference.offset, PAGE_SIZE, "persistent replacement page")
        .map_err(PersistentSourceReplacementError::Source)?;
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(page.len())
                .map_err(|_| PersistentSourceReplacementError::Source(
                    ImmutableSourceError::Limit("hashed bytes"),
                ))?,
        )
        .ok_or(PersistentSourceReplacementError::Source(
            ImmutableSourceError::Limit("hashed bytes"),
        ))?;
    if digest(&[PAGE_DOMAIN, &page]) != reference.digest || &page[..8] != PAGE_MAGIC {
        return Err(PersistentSourceReplacementError::Source(
            ImmutableSourceError::Format(ImmutableError::Invalid("persistent page digest")),
        ));
    }

    let kind = page[8];
    let level = page[9];
    let count = usize::try_from(
        u32_at(&page, 12, "persistent page count")
            .map_err(PersistentSourceReplacementError::Writer)?,
    )
    .map_err(|_| PersistentSourceReplacementError::Writer(ImmutableError::Invalid(
        "persistent page count",
    )))?;
    let minimum = u64_at(&page, 20, "persistent page minimum")
        .map_err(PersistentSourceReplacementError::Writer)?;
    let maximum = u64_at(&page, 28, "persistent page maximum")
        .map_err(PersistentSourceReplacementError::Writer)?;
    if level != reference.level
        || reference
            .range
            .is_some_and(|range| range != (minimum, maximum))
    {
        return Err(PersistentSourceReplacementError::Writer(
            ImmutableError::Invalid("persistent page reference"),
        ));
    }

    match kind {
        1 => {
            if level != 0
                || count == 0
                || count > LEAF_CAPACITY
                || usize::try_from(
                    u32_at(&page, 16, "persistent leaf entry size")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                )
                .map_err(|_| PersistentSourceReplacementError::Writer(
                    ImmutableError::Invalid("persistent leaf entry size"),
                ))?
                    != LEAF_ENTRY_LEN
            {
                return Err(PersistentSourceReplacementError::Writer(
                    ImmutableError::Invalid("persistent leaf"),
                ));
            }
            allocation_check::<Locator>(count, reader.limits.format)
                .map_err(PersistentSourceReplacementError::Writer)?;
            let mut entries = Vec::with_capacity(count);
            let mut changed = false;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let locator = Locator {
                    object_id: u64_at(&page, entry, "persistent leaf entry")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                    kind: u16_at(&page, entry + 8, "persistent leaf entry")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                    record_offset: u64_at(&page, entry + 16, "persistent leaf entry")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                    record_len: u64_at(&page, entry + 24, "persistent leaf entry")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                    logical_len: u64_at(&page, entry + 32, "persistent leaf entry")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                    digest: array(&page, entry + 40, "persistent leaf entry")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                };
                if let Some(replacement) = replacements.get(&locator.object_id) {
                    entries.push(replacement.clone());
                    changed = true;
                } else {
                    entries.push(locator);
                }
            }
            if !changed {
                return Ok((
                    PageRef {
                        minimum,
                        maximum,
                        offset: u64::try_from(reference.offset).map_err(|_| {
                            PersistentSourceReplacementError::Source(
                                ImmutableSourceError::Limit("offset"),
                            )
                        })?,
                        level,
                        digest: reference.digest,
                    },
                    false,
                ));
            }
            let encoded = encode_leaf(&entries).map_err(PersistentSourceReplacementError::Writer)?;
            let page_ref = append_persistent_tail_page(
                tail,
                base_len,
                &encoded,
                reader.limits.format,
                pages_written,
            )
            .map_err(PersistentSourceReplacementError::Writer)?;
            Ok((page_ref, true))
        }
        2 => {
            if level == 0
                || count == 0
                || count > INTERNAL_FANOUT
                || usize::try_from(
                    u32_at(&page, 16, "persistent internal entry size")
                        .map_err(PersistentSourceReplacementError::Writer)?,
                )
                .map_err(|_| PersistentSourceReplacementError::Writer(
                    ImmutableError::Invalid("persistent internal entry size"),
                ))?
                    != INTERNAL_ENTRY_LEN
            {
                return Err(PersistentSourceReplacementError::Writer(
                    ImmutableError::Invalid("persistent internal"),
                ));
            }
            allocation_check::<PageRef>(count, reader.limits.format)
                .map_err(PersistentSourceReplacementError::Writer)?;
            let mut children = Vec::with_capacity(count);
            let mut changed = false;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child_minimum = u64_at(&page, entry, "persistent child")
                    .map_err(PersistentSourceReplacementError::Writer)?;
                let child_maximum = u64_at(&page, entry + 8, "persistent child")
                    .map_err(PersistentSourceReplacementError::Writer)?;
                let child_offset = usize_at(&page, entry + 16, "persistent child")
                    .map_err(PersistentSourceReplacementError::Writer)?;
                let child_digest = array(&page, entry + 32, "persistent child")
                    .map_err(PersistentSourceReplacementError::Writer)?;
                let child_reference = LookupReference {
                    offset: child_offset,
                    level: level - 1,
                    digest: child_digest,
                    range: Some((child_minimum, child_maximum)),
                };
                if replacements
                    .range(child_minimum..=child_maximum)
                    .next()
                    .is_some()
                {
                    let (next, child_changed) = persistent_source_replacement_page(
                        reader,
                        envelope,
                        &child_reference,
                        replacements,
                        tail,
                        base_len,
                        pages_written,
                    )?;
                    children.push(next);
                    changed |= child_changed;
                } else {
                    children.push(PageRef {
                        minimum: child_minimum,
                        maximum: child_maximum,
                        offset: u64::try_from(child_offset).map_err(|_| {
                            PersistentSourceReplacementError::Source(
                                ImmutableSourceError::Limit("offset"),
                            )
                        })?,
                        level: level - 1,
                        digest: child_digest,
                    });
                }
            }
            if !changed {
                return Ok((
                    PageRef {
                        minimum,
                        maximum,
                        offset: u64::try_from(reference.offset).map_err(|_| {
                            PersistentSourceReplacementError::Source(
                                ImmutableSourceError::Limit("offset"),
                            )
                        })?,
                        level,
                        digest: reference.digest,
                    },
                    false,
                ));
            }
            let encoded = encode_internal(&children, level)
                .map_err(PersistentSourceReplacementError::Writer)?;
            let page_ref = append_persistent_tail_page(
                tail,
                base_len,
                &encoded,
                reader.limits.format,
                pages_written,
            )
            .map_err(PersistentSourceReplacementError::Writer)?;
            Ok((page_ref, true))
        }
        _ => Err(PersistentSourceReplacementError::Writer(
            ImmutableError::Invalid("persistent page kind"),
        )),
    }
}

fn plan_persistent_source_replacements_inner<S: ImmutableReadAt>(
    source: &mut S,
    operations: &[ImmutableBatchOperation],
    limits: ImmutableSourceLimits,
) -> Result<
    (
        PersistentSourceIdentity,
        Vec<u8>,
        ImmutableReport,
        usize,
        usize,
        ImmutableSourceStats,
    ),
    PersistentSourceReplacementError,
> {
    if operations.is_empty() {
        return Err(PersistentSourceReplacementError::Writer(
            ImmutableError::Invalid("batch operations"),
        ));
    }
    allocation_check::<usize>(operations.len(), limits.format)
        .map_err(PersistentSourceReplacementError::Writer)?;
    let mut order: Vec<usize> = (0..operations.len()).collect();
    order.sort_unstable_by_key(|index| operations[*index].object_id());
    if let Some(pair) = order.windows(2).find(|pair| {
        operations[pair[0]].object_id() == operations[pair[1]].object_id()
    }) {
        return Err(PersistentSourceReplacementError::Writer(
            ImmutableError::DuplicateObject(operations[pair[0]].object_id()),
        ));
    }
    for index in &order {
        let ImmutableBatchOperation::Put(input) = &operations[*index] else {
            return Err(PersistentSourceReplacementError::Writer(
                ImmutableError::Invalid("persistent replacement operations"),
            ));
        };
        if input.object_id == 0 || input.kind == 0 {
            return Err(PersistentSourceReplacementError::Writer(
                ImmutableError::Invalid("object identity"),
            ));
        }
    }

    let strict = validate_source_at(source, limits)
        .map_err(PersistentSourceReplacementError::Source)?;
    let mut total_stats = strict.stats;
    let identity_limits = remaining_source_limits(limits, total_stats)
        .map_err(PersistentSourceReplacementError::Source)?;
    let (identity, identity_stats) = persistent_source_identity(source, identity_limits)
        .map_err(PersistentSourceReplacementError::Source)?;
    add_source_stats(&mut total_stats, identity_stats)
        .map_err(PersistentSourceReplacementError::Source)?;

    let path_limits = remaining_source_limits(limits, total_stats)
        .map_err(PersistentSourceReplacementError::Source)?;
    let mut reader = SourceReader::new(source, path_limits)
        .map_err(PersistentSourceReplacementError::Source)?;
    if u64::try_from(reader.length).map_err(|_| PersistentSourceReplacementError::Source(
        ImmutableSourceError::Limit("length"),
    ))? != identity.length
    {
        return Err(PersistentSourceReplacementError::Source(
            ImmutableSourceError::Format(ImmutableError::Invalid("source length")),
        ));
    }
    let envelope = read_lookup_envelope(&mut reader)
        .map_err(PersistentSourceReplacementError::Source)?;
    if envelope.sequence != strict.report.sequence
        || envelope.snapshot_digest != strict.report.snapshot_digest
        || envelope.commit_digest != strict.report.commit_digest
    {
        return Err(PersistentSourceReplacementError::Source(
            ImmutableSourceError::Format(ImmutableError::Invalid("source report")),
        ));
    }

    let base_len = reader.length;
    let mut tail = Vec::new();
    let mut replacements = std::collections::BTreeMap::new();
    for index in order {
        let ImmutableBatchOperation::Put(input) = &operations[index] else {
            unreachable!("replacement-only operations validated above");
        };
        let locator = append_persistent_tail_object(&mut tail, base_len, input, limits.format)
            .map_err(PersistentSourceReplacementError::Writer)?;
        replacements.insert(input.object_id, locator);
    }

    let mut pages_written = 0_usize;
    let (root, changed) = persistent_source_replacement_page(
        &mut reader,
        &envelope,
        &envelope.root,
        &replacements,
        &mut tail,
        base_len,
        &mut pages_written,
    )?;
    if !changed || pages_written == 0 {
        let missing = replacements
            .keys()
            .next()
            .copied()
            .ok_or(PersistentSourceReplacementError::Writer(
                ImmutableError::Invalid("persistent replacement state"),
            ))?;
        return Err(PersistentSourceReplacementError::Writer(
            ImmutableError::MissingObject(missing),
        ));
    }
    let pages_reused = strict
        .report
        .page_count
        .checked_sub(pages_written)
        .ok_or(PersistentSourceReplacementError::Writer(
            ImmutableError::Invalid("persistent page accounting"),
        ))?;
    let publication = PersistentTailPublication {
        base_len,
        sequence: strict
            .report
            .sequence
            .checked_add(1)
            .ok_or(PersistentSourceReplacementError::Writer(
                ImmutableError::Limit("sequence"),
            ))?,
        root,
        parent_snapshot_digest: strict.report.snapshot_digest,
        previous_footer_offset: u64::try_from(envelope.footer_offset).map_err(|_| {
            PersistentSourceReplacementError::Source(ImmutableSourceError::Limit("offset"))
        })?,
        page_count: pages_written,
        object_count: strict.report.object_count,
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits.format)
        .map_err(PersistentSourceReplacementError::Writer)?;
    report.page_count = strict.report.page_count;
    persistent_tail_total_len(base_len, tail.len(), limits.format)
        .map_err(PersistentSourceReplacementError::Writer)?;

    add_source_stats(&mut total_stats, reader.stats)
        .map_err(PersistentSourceReplacementError::Source)?;
    Ok((
        identity,
        tail,
        report,
        pages_written,
        pages_reused,
        total_stats,
    ))
}

/// Plans a replacement-only persistent append tail directly from one strongly versioned bounded
/// random-access source.
///
/// The operation strictly validates the exact-end source, hashes the complete file identity, then
/// rereads only replacement paths to construct the same tail as the in-memory replacement writer.
/// It retains no complete base copy. Current commit authentication and strict validation still read
/// all active data; this experiment proves bounded memory and path-local tail construction, not
/// minimal source traffic.
pub fn plan_persistent_replacement_tail_at<S: PersistentVersionedReadAt>(
    source: &mut S,
    operations: &[ImmutableBatchOperation],
    limits: ImmutableSourceLimits,
) -> Result<PersistentSourceReplacementPlan, PersistentSourceReplacementError> {
    let version = source
        .version_token()
        .map_err(PersistentSourceReplacementError::Version)?;
    let mut stable = PersistentReplacementStableSource::new(source, version);
    let result = plan_persistent_source_replacements_inner(&mut stable, operations, limits);
    if stable.changed {
        return Err(PersistentSourceReplacementError::VersionChanged);
    }
    if let Some(error) = stable.version_error {
        return Err(PersistentSourceReplacementError::Version(error));
    }
    let (identity, tail, report, pages_written, pages_reused, source_stats) = result?;
    Ok(PersistentSourceReplacementPlan {
        identity,
        version,
        tail_allocation_bytes: tail.capacity(),
        tail,
        report,
        pages_written,
        pages_reused,
        source_stats,
        version_checks: stable.version_checks,
    })
}

#[cfg(test)]
mod persistent_source_replacement_tests {
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
            max_total_bytes_read: u64::try_from(file_len * 8).expect("budget"),
            max_read_operations: 2_000_000,
            max_read_request_bytes: 257,
            hash_block_bytes: 251,
        }
    }

    #[test]
    fn source_planned_tail_matches_owned_replacement_writer() {
        let format = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = base(220, format);
        let operations = vec![
            ImmutableBatchOperation::Put(object(2, 211, 17)),
            ImmutableBatchOperation::Put(object(440, 212, 19)),
        ];
        let owned = append_persistent_batch(&base, &operations, format).expect("owned");
        let mut source = VersionedSlice {
            bytes: base.clone(),
            version: PersistentSourceVersion([21; 32]),
            reads: 0,
            mutate_after_read: None,
        };
        let plan = plan_persistent_replacement_tail_at(
            &mut source,
            &operations,
            source_limits(format, base.len()),
        )
        .expect("source plan");
        assert_eq!(plan.tail, owned.bytes[base.len()..]);
        assert_eq!(plan.report, owned.report);
        assert_eq!(plan.pages_written, owned.pages_written);
        assert_eq!(plan.pages_reused, owned.pages_reused);
        assert_eq!(plan.identity, PersistentSourceIdentity::from_bytes(&base).expect("identity"));
        assert!(plan.version_checks > 0);
        assert!(plan.tail_allocation_bytes < owned.bytes.len());
    }

    #[test]
    fn caller_order_cannot_change_source_planned_tail() {
        let format = ImmutableLimits::default();
        let base = base(120, format);
        let forward = vec![
            ImmutableBatchOperation::Put(object(2, 201, 11)),
            ImmutableBatchOperation::Put(object(240, 202, 13)),
        ];
        let mut reverse = forward.clone();
        reverse.reverse();
        let mut first_source = VersionedSlice {
            bytes: base.clone(),
            version: PersistentSourceVersion([22; 32]),
            reads: 0,
            mutate_after_read: None,
        };
        let mut second_source = VersionedSlice {
            bytes: base.clone(),
            version: PersistentSourceVersion([22; 32]),
            reads: 0,
            mutate_after_read: None,
        };
        let first = plan_persistent_replacement_tail_at(
            &mut first_source,
            &forward,
            source_limits(format, base.len()),
        )
        .expect("forward");
        let second = plan_persistent_replacement_tail_at(
            &mut second_source,
            &reverse,
            source_limits(format, base.len()),
        )
        .expect("reverse");
        assert_eq!(first.tail, second.tail);
        assert_eq!(first.report, second.report);
    }

    #[test]
    fn source_version_change_rejects_without_a_plan() {
        let format = ImmutableLimits::default();
        let base = base(16, format);
        let mut source = VersionedSlice {
            bytes: base.clone(),
            version: PersistentSourceVersion([23; 32]),
            reads: 0,
            mutate_after_read: Some(1),
        };
        let error = plan_persistent_replacement_tail_at(
            &mut source,
            &[ImmutableBatchOperation::Put(object(2, 203, 9))],
            source_limits(format, base.len()),
        )
        .expect_err("changed version");
        assert_eq!(error, PersistentSourceReplacementError::VersionChanged);
    }

    #[test]
    fn cumulative_budget_exhaustion_is_reported() {
        let format = ImmutableLimits::default();
        let base = base(16, format);
        let mut source = VersionedSlice {
            bytes: base.clone(),
            version: PersistentSourceVersion([24; 32]),
            reads: 0,
            mutate_after_read: None,
        };
        let error = plan_persistent_replacement_tail_at(
            &mut source,
            &[ImmutableBatchOperation::Put(object(2, 204, 9))],
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
            PersistentSourceReplacementError::Source(ImmutableSourceError::Limit(_))
        ));
    }
}
