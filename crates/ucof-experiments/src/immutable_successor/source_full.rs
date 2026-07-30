#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceStrictReport {
    pub report: ImmutableReport,
    pub stats: ImmutableSourceStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceHistoryReport {
    pub history: ImmutableHistoryReport,
    pub stats: ImmutableSourceStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceRecoveryReport {
    pub recovery: ImmutableRecoveryReport,
    pub stats: ImmutableSourceStats,
}

struct PrefixSource<'a, S> {
    inner: &'a mut S,
    length: u64,
    limits: ImmutableSourceLimits,
    stats: ImmutableSourceStats,
}

impl<S: ImmutableReadAt> ImmutableReadAt for PrefixSource<'_, S> {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        Ok(self.length)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        let length = u64::try_from(buffer.len())
            .map_err(|_| ImmutableSourceError::Limit("read bytes"))?;
        let end = offset
            .checked_add(length)
            .ok_or(ImmutableSourceError::Io("range"))?;
        if end > self.length || buffer.len() > self.limits.max_read_request_bytes {
            return Err(ImmutableSourceError::Io("range"));
        }
        let next_operations = self
            .stats
            .read_operations
            .checked_add(1)
            .ok_or(ImmutableSourceError::Limit("read operations"))?;
        let next_bytes = self
            .stats
            .bytes_read
            .checked_add(length)
            .ok_or(ImmutableSourceError::Limit("read bytes"))?;
        if next_operations > self.limits.max_read_operations {
            return Err(ImmutableSourceError::Limit("read operations"));
        }
        if next_bytes > self.limits.max_total_bytes_read {
            return Err(ImmutableSourceError::Limit("read bytes"));
        }
        self.stats.read_operations = next_operations;
        self.stats.bytes_read = next_bytes;
        self.inner.read_exact_at(offset, buffer)
    }
}

fn add_source_stats(
    total: &mut ImmutableSourceStats,
    addition: ImmutableSourceStats,
) -> Result<(), ImmutableSourceError> {
    total.read_operations = total
        .read_operations
        .checked_add(addition.read_operations)
        .ok_or(ImmutableSourceError::Limit("read operations"))?;
    total.bytes_read = total
        .bytes_read
        .checked_add(addition.bytes_read)
        .ok_or(ImmutableSourceError::Limit("read bytes"))?;
    total.bytes_hashed = total
        .bytes_hashed
        .checked_add(addition.bytes_hashed)
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    total.largest_allocation = total.largest_allocation.max(addition.largest_allocation);
    Ok(())
}

fn remaining_source_limits(
    limits: ImmutableSourceLimits,
    stats: ImmutableSourceStats,
) -> Result<ImmutableSourceLimits, ImmutableSourceError> {
    Ok(ImmutableSourceLimits {
        max_total_bytes_read: limits
            .max_total_bytes_read
            .checked_sub(stats.bytes_read)
            .ok_or(ImmutableSourceError::Limit("read bytes"))?,
        max_read_operations: limits
            .max_read_operations
            .checked_sub(stats.read_operations)
            .ok_or(ImmutableSourceError::Limit("read operations"))?,
        ..limits
    })
}

fn read_direct<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
    stats: &mut ImmutableSourceStats,
    offset: u64,
    buffer: &mut [u8],
) -> Result<(), ImmutableSourceError> {
    let mut completed = 0_usize;
    while completed < buffer.len() {
        let take = (buffer.len() - completed).min(limits.max_read_request_bytes);
        if take == 0 || stats.read_operations >= limits.max_read_operations {
            return Err(ImmutableSourceError::Limit("read operations"));
        }
        let take_u64 = u64::try_from(take)
            .map_err(|_| ImmutableSourceError::Limit("read bytes"))?;
        let next_total = stats
            .bytes_read
            .checked_add(take_u64)
            .ok_or(ImmutableSourceError::Limit("read bytes"))?;
        if next_total > limits.max_total_bytes_read {
            return Err(ImmutableSourceError::Limit("read bytes"));
        }
        let completed_u64 = u64::try_from(completed)
            .map_err(|_| ImmutableSourceError::Limit("offset"))?;
        source.read_exact_at(
            offset
                .checked_add(completed_u64)
                .ok_or(ImmutableSourceError::Limit("offset"))?,
            &mut buffer[completed..completed + take],
        )?;
        stats.read_operations += 1;
        stats.bytes_read = next_total;
        completed += take;
    }
    Ok(())
}

fn read_full_page<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    reference: &LookupReference,
    envelope: &LookupEnvelope,
    visited: &mut HashSet<usize>,
    stack: &mut Vec<LookupReference>,
    locators: &mut Vec<Locator>,
    known_ranges: &mut Vec<(usize, usize)>,
) -> Result<(), ImmutableSourceError> {
    if visited.len() >= reader.limits.format.max_pages {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit(
            "page count",
        )));
    }
    if !visited.insert(reference.offset) {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page cycle",
        )));
    }
    if !known_ranges
        .iter()
        .any(|range| *range == (reference.offset, reference.offset + PAGE_SIZE))
    {
        register_page_range(known_ranges, reference.offset, envelope.snapshot_offset)?;
    }

    let page = reader.read_vec(reference.offset, PAGE_SIZE, "page")?;
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(page.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    if digest(&[PAGE_DOMAIN, &page]) != reference.digest || &page[..8] != PAGE_MAGIC {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page digest",
        )));
    }

    let kind = page[8];
    let level = page[9];
    let reserved = u16_at(&page, 10, "page header")?;
    let count = usize::try_from(u32_at(&page, 12, "page header")?)
        .map_err(|_| ImmutableSourceError::Format(ImmutableError::Invalid("page count")))?;
    let entry_size = usize::try_from(u32_at(&page, 16, "page header")?)
        .map_err(|_| ImmutableSourceError::Format(ImmutableError::Invalid("page entry size")))?;
    let minimum = u64_at(&page, 20, "page header")?;
    let maximum = u64_at(&page, 28, "page header")?;
    if reserved != 0 || page[36..PAGE_HEADER_LEN].iter().any(|byte| *byte != 0) || count == 0 {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page header",
        )));
    }
    if level != reference.level
        || reference
            .range
            .is_some_and(|range| range != (minimum, maximum))
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page reference",
        )));
    }

    match kind {
        1 => {
            if level != 0 || entry_size != LEAF_ENTRY_LEN || count > LEAF_CAPACITY {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "leaf shape",
                )));
            }
            if locators
                .len()
                .checked_add(count)
                .is_none_or(|value| value > reader.limits.format.max_objects)
            {
                return Err(ImmutableSourceError::Format(ImmutableError::Limit(
                    "object count",
                )));
            }
            allocation_check::<Locator>(locators.len() + count, reader.limits.format)?;
            let mut previous = None;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let object_id = u64_at(&page, entry, "leaf entry")?;
                let object_kind = u16_at(&page, entry + 8, "leaf entry")?;
                if object_id == 0
                    || object_kind == 0
                    || page[entry + 10..entry + 16].iter().any(|byte| *byte != 0)
                    || page[entry + 72..entry + 88].iter().any(|byte| *byte != 0)
                    || previous.is_some_and(|value| value >= object_id)
                {
                    return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                        "leaf entry",
                    )));
                }
                previous = Some(object_id);
                locators.push(Locator {
                    object_id,
                    kind: object_kind,
                    record_offset: u64_at(&page, entry + 16, "leaf entry")?,
                    record_len: u64_at(&page, entry + 24, "leaf entry")?,
                    logical_len: u64_at(&page, entry + 32, "leaf entry")?,
                    digest: array(&page, entry + 40, "leaf entry")?,
                });
            }
            if u64_at(&page, PAGE_HEADER_LEN, "leaf order")? != minimum
                || previous != Some(maximum)
                || page[PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN..]
                    .iter()
                    .any(|byte| *byte != 0)
            {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "leaf order",
                )));
            }
        }
        2 => {
            if level == 0 || entry_size != INTERNAL_ENTRY_LEN || count > INTERNAL_FANOUT {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "internal shape",
                )));
            }
            if stack
                .len()
                .checked_add(count)
                .is_none_or(|value| value > reader.limits.format.max_pages)
            {
                return Err(ImmutableSourceError::Format(ImmutableError::Limit(
                    "page count",
                )));
            }
            allocation_check::<LookupReference>(stack.len() + count, reader.limits.format)?;
            let mut previous_maximum = None;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child_minimum = u64_at(&page, entry, "child entry")?;
                let child_maximum = u64_at(&page, entry + 8, "child entry")?;
                let child_offset = usize_at(&page, entry + 16, "child entry")?;
                let child_len = usize_at(&page, entry + 24, "child entry")?;
                if child_minimum > child_maximum
                    || child_len != PAGE_SIZE
                    || previous_maximum.is_some_and(|value| value >= child_minimum)
                {
                    return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                        "child entry",
                    )));
                }
                previous_maximum = Some(child_maximum);
                register_page_range(known_ranges, child_offset, envelope.snapshot_offset)?;
                stack.push(LookupReference {
                    offset: child_offset,
                    level: level - 1,
                    digest: array(&page, entry + 32, "child entry")?,
                    range: Some((child_minimum, child_maximum)),
                });
            }
            if u64_at(&page, PAGE_HEADER_LEN, "child order")? != minimum
                || previous_maximum != Some(maximum)
                || page[PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN..]
                    .iter()
                    .any(|byte| *byte != 0)
            {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "child order",
                )));
            }
        }
        _ => {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                "page kind",
            )));
        }
    }
    Ok(())
}

/// Strictly validates the exact-end active snapshot through a bounded random-access source.
pub fn validate_source_at<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceStrictReport, ImmutableSourceError> {
    let mut reader = SourceReader::new(source, limits)?;
    let envelope = read_lookup_envelope(&mut reader)?;
    let footer_raw = reader.read_vec(envelope.footer_offset, FOOTER_LEN, "footer")?;
    let footer = parse_footer(&footer_raw, 0)?;
    let commit_start = if footer.previous_footer_offset == ABSENT_OFFSET {
        0
    } else {
        usize_from_u64(footer.previous_footer_offset, "previous footer")?
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "previous footer",
            )))?
    };

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

    let current_pages = visited
        .iter()
        .filter(|offset| **offset >= commit_start)
        .count();
    if footer.page_count_current != u64_from_usize(current_pages)? {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page count",
        )));
    }
    locators.sort_by_key(|locator| locator.object_id);
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object order",
        )));
    }

    allocation_check::<(usize, usize)>(locators.len(), reader.limits.format)?;
    let mut object_ranges = Vec::with_capacity(locators.len());
    for locator in &locators {
        let offset = usize_from_u64(locator.record_offset, "object range")?;
        let length = usize_from_u64(locator.record_len, "object range")?;
        let end = offset
            .checked_add(length)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "object range",
            )))?;
        object_ranges.push((offset, end));
    }
    object_ranges.sort_unstable();
    if object_ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object overlap",
        )));
    }
    for locator in &locators {
        let result = validate_lookup_object(&mut reader, locator, &envelope, &known_ranges)?;
        if !matches!(result, ImmutableLookupResult::Found { .. }) {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                "object",
            )));
        }
    }

    Ok(ImmutableSourceStrictReport {
        report: ImmutableReport {
            sequence: envelope.sequence,
            object_count: locators.len(),
            page_count: visited.len(),
            root_level: envelope.root.level,
            snapshot_digest: envelope.snapshot_digest,
            commit_digest: envelope.commit_digest,
        },
        stats: reader.stats,
    })
}

fn validate_source_prefix<S: ImmutableReadAt>(
    source: &mut S,
    prefix_len: u64,
    limits: ImmutableSourceLimits,
    stats: &mut ImmutableSourceStats,
) -> Result<ImmutableSourceStrictReport, ImmutableSourceError> {
    let call_limits = remaining_source_limits(limits, *stats)?;
    let mut prefix = PrefixSource {
        inner: source,
        length: prefix_len,
        limits: call_limits,
        stats: ImmutableSourceStats::default(),
    };
    let result = validate_source_at(&mut prefix, call_limits);
    let attempted = prefix.stats;
    match result {
        Ok(mut report) => {
            let addition = ImmutableSourceStats {
                read_operations: attempted.read_operations,
                bytes_read: attempted.bytes_read,
                bytes_hashed: report.stats.bytes_hashed,
                largest_allocation: report
                    .stats
                    .largest_allocation
                    .max(attempted.largest_allocation),
            };
            add_source_stats(stats, addition)?;
            report.stats = addition;
            Ok(report)
        }
        Err(error) => {
            add_source_stats(stats, attempted)?;
            Err(error)
        }
    }
}

fn source_footer_and_parent<S: ImmutableReadAt>(
    source: &mut S,
    prefix_len: u64,
    limits: ImmutableSourceLimits,
    stats: &mut ImmutableSourceStats,
) -> Result<(Footer, [u8; 32]), ImmutableSourceError> {
    let footer_offset = prefix_len
        .checked_sub(u64::try_from(FOOTER_LEN).expect("footer length"))
        .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
            "file length",
        )))?;
    let mut footer_raw = vec![0_u8; FOOTER_LEN];
    stats.largest_allocation = stats.largest_allocation.max(footer_raw.len());
    read_direct(source, limits, stats, footer_offset, &mut footer_raw)?;
    let footer = parse_footer(&footer_raw, 0)?;
    if footer
        .snapshot_offset
        .checked_add(footer.snapshot_len)
        .is_none_or(|end| end != footer_offset)
        || footer.snapshot_len != u64::try_from(SNAPSHOT_LEN).expect("snapshot length")
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "snapshot range",
        )));
    }
    let mut snapshot = vec![0_u8; SNAPSHOT_LEN];
    stats.largest_allocation = stats.largest_allocation.max(snapshot.len());
    read_direct(
        source,
        limits,
        stats,
        footer.snapshot_offset,
        &mut snapshot,
    )?;
    if digest(&[SNAPSHOT_DOMAIN, &snapshot]) != footer.snapshot_digest {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "snapshot digest",
        )));
    }
    Ok((footer, array(&snapshot, 64, "snapshot parent")?))
}

/// Revalidates every linked exact prefix through a bounded random-access source.
pub fn validate_source_history<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceHistoryReport, ImmutableSourceError> {
    let mut prefix_len = source.len()?;
    let mut stats = ImmutableSourceStats::default();
    let mut entries = Vec::new();
    let mut expected = None;

    loop {
        if entries.len() >= limits.format.max_history_entries {
            return Err(ImmutableSourceError::Format(ImmutableError::Limit(
                "history entries",
            )));
        }
        allocation_check::<ImmutableHistoryEntry>(entries.len() + 1, limits.format)?;
        let strict = validate_source_prefix(source, prefix_len, limits, &mut stats)?;
        if let Some((sequence, snapshot_digest)) = expected {
            if strict.report.sequence != sequence
                || strict.report.snapshot_digest != snapshot_digest
            {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "parent linkage",
                )));
            }
        }
        let footer_offset = prefix_len
            .checked_sub(u64::try_from(FOOTER_LEN).expect("footer length"))
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "file length",
            )))?;
        let (footer, parent_digest) =
            source_footer_and_parent(source, prefix_len, limits, &mut stats)?;
        entries.push(ImmutableHistoryEntry {
            footer_offset,
            report: strict.report.clone(),
        });
        if footer.previous_footer_offset == ABSENT_OFFSET {
            if footer.sequence != 0 || parent_digest.iter().any(|byte| *byte != 0) {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "genesis linkage",
                )));
            }
            break;
        }
        if footer.sequence == 0 || footer.previous_footer_offset >= footer_offset {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                "previous footer",
            )));
        }
        expected = Some((footer.sequence - 1, parent_digest));
        prefix_len = footer
            .previous_footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).expect("footer length"))
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "previous footer",
            )))?;
    }

    Ok(ImmutableSourceHistoryReport {
        history: ImmutableHistoryReport { entries },
        stats,
    })
}

/// Scans a bounded suffix and reports strictly validated source prefixes without selecting one.
pub fn scan_source_recovery<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceRecoveryReport, ImmutableSourceError> {
    let length = source.len()?;
    let scan_len = usize::try_from(
        length.min(
            u64::try_from(limits.format.max_recovery_scan_bytes)
                .map_err(|_| ImmutableSourceError::Limit("recovery scan"))?,
        ),
    )
    .map_err(|_| ImmutableSourceError::Limit("recovery scan"))?;
    if scan_len > limits.format.max_allocation_bytes {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit(
            "allocation",
        )));
    }
    let scan_start = length
        .checked_sub(u64::try_from(scan_len).map_err(|_| ImmutableSourceError::Limit("recovery scan"))?)
        .ok_or(ImmutableSourceError::Limit("recovery scan"))?;
    let mut stats = ImmutableSourceStats {
        largest_allocation: scan_len,
        ..ImmutableSourceStats::default()
    };
    let mut suffix = vec![0_u8; scan_len];
    read_direct(source, limits, &mut stats, scan_start, &mut suffix)?;

    let mut offsets = Vec::new();
    if suffix.len() >= FOOTER_MAGIC.len() {
        for index in 0..=suffix.len() - FOOTER_MAGIC.len() {
            if &suffix[index..index + FOOTER_MAGIC.len()] == FOOTER_MAGIC {
                offsets.push(
                    scan_start
                        .checked_add(
                            u64::try_from(index)
                                .map_err(|_| ImmutableSourceError::Limit("offset"))?,
                        )
                        .ok_or(ImmutableSourceError::Limit("offset"))?,
                );
            }
        }
    }
    offsets.reverse();

    let mut attempted_footers = 0_usize;
    let mut attempts_truncated = false;
    let mut candidates_truncated = false;
    let mut candidates = Vec::new();
    for footer_offset in offsets {
        if attempted_footers >= limits.format.max_recovery_attempts {
            attempts_truncated = true;
            break;
        }
        attempted_footers += 1;
        let prefix_len = match footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).expect("footer length"))
        {
            Some(value) if value <= length => value,
            _ => continue,
        };
        match validate_source_prefix(source, prefix_len, limits, &mut stats) {
            Ok(strict) => {
                if candidates.len() >= limits.format.max_recovery_candidates {
                    candidates_truncated = true;
                    break;
                }
                allocation_check::<ImmutableRecoveryCandidate>(
                    candidates.len() + 1,
                    limits.format,
                )?;
                candidates.push(ImmutableRecoveryCandidate {
                    footer_offset,
                    prefix_len,
                    report: strict.report,
                });
            }
            Err(ImmutableSourceError::Format(_)) => {}
            Err(error) => return Err(error),
        }
    }

    Ok(ImmutableSourceRecoveryReport {
        recovery: ImmutableRecoveryReport {
            scan_start,
            scanned_bytes: scan_len,
            attempted_footers,
            attempts_truncated,
            candidates_truncated,
            candidates,
        },
        stats,
    })
}
