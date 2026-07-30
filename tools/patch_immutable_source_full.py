#!/usr/bin/env python3
"""Apply bounded source-history hardening before read-only verification."""

from pathlib import Path

path = Path("crates/ucof-experiments/src/immutable_successor/source_full.rs")
text = path.read_text(encoding="utf-8")

old = '''struct PrefixSource<'a, S> {
    inner: &'a mut S,
    length: u64,
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
        if end > self.length {
            return Err(ImmutableSourceError::Io("range"));
        }
        self.inner.read_exact_at(offset, buffer)
    }
}
'''
new = '''struct PrefixSource<'a, S> {
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
'''
if old not in text:
    raise SystemExit("PrefixSource anchor not found")
text = text.replace(old, new, 1)

old = '''    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
'''
new = '''    locators.sort_by_key(|locator| locator.object_id);
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
'''
if old not in text:
    raise SystemExit("locator ordering anchor not found")
text = text.replace(old, new, 1)

old = '''    let mut object_ranges = Vec::with_capacity(locators.len());
    allocation_check::<(usize, usize)>(locators.len(), reader.limits.format)?;
'''
new = '''    allocation_check::<(usize, usize)>(locators.len(), reader.limits.format)?;
    let mut object_ranges = Vec::with_capacity(locators.len());
'''
if old not in text:
    raise SystemExit("object range allocation anchor not found")
text = text.replace(old, new, 1)

old = '''fn validate_source_prefix<S: ImmutableReadAt>(
    source: &mut S,
    prefix_len: u64,
    limits: ImmutableSourceLimits,
    stats: &mut ImmutableSourceStats,
) -> Result<ImmutableSourceStrictReport, ImmutableSourceError> {
    let call_limits = remaining_source_limits(limits, *stats)?;
    let mut prefix = PrefixSource {
        inner: source,
        length: prefix_len,
    };
    let report = validate_source_at(&mut prefix, call_limits)?;
    add_source_stats(stats, report.stats)?;
    Ok(report)
}
'''
new = '''fn validate_source_prefix<S: ImmutableReadAt>(
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
'''
if old not in text:
    raise SystemExit("prefix validation anchor not found")
text = text.replace(old, new, 1)

old = '''    let mut offsets = Vec::new();
    for index in 0..=suffix.len().saturating_sub(FOOTER_MAGIC.len()) {
        if &suffix[index..index + FOOTER_MAGIC.len()] == FOOTER_MAGIC {
            offsets.push(
                scan_start
                    .checked_add(u64::try_from(index).map_err(|_| ImmutableSourceError::Limit("offset"))?)
                    .ok_or(ImmutableSourceError::Limit("offset"))?,
            );
        }
    }
'''
new = '''    let mut offsets = Vec::new();
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
'''
if old not in text:
    raise SystemExit("suffix scan anchor not found")
text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")
