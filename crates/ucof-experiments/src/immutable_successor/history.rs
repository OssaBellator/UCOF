/// Strictly validates the exact-end commit and every linked historical prefix.
///
/// Entries are returned newest first. This function never searches for alternate
/// roots and never treats a damaged historical prefix as valid.
pub fn validate_history(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<ImmutableHistoryReport, ImmutableError> {
    if limits.max_history_entries == 0 {
        return Err(ImmutableError::Limit("history entries"));
    }
    let mut end = data.len();
    let mut entries = Vec::new();
    loop {
        if entries.len() >= limits.max_history_entries {
            return Err(ImmutableError::Limit("history entries"));
        }
        allocation_check::<ImmutableHistoryEntry>(entries.len() + 1, limits)?;
        let internal = validate_internal(&data[..end], limits)?;
        let footer = parse_footer(&data[..end], internal.footer_offset)?;
        let previous_footer_offset = footer.previous_footer_offset;
        entries.push(ImmutableHistoryEntry {
            footer_offset: u64_from_usize(internal.footer_offset)?,
            report: internal.public,
        });
        if previous_footer_offset == ABSENT_OFFSET {
            break;
        }
        let previous_offset = usize_from_u64(previous_footer_offset, "history footer")?;
        end = previous_offset
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableError::Invalid("history footer"))?;
        if end >= data.len() || end > internal.footer_offset {
            return Err(ImmutableError::Invalid("history linkage"));
        }
    }
    Ok(ImmutableHistoryReport { entries })
}

/// Scans only the configured suffix and reports strictly validated prefixes.
///
/// The result is evidence, not a selection decision. Invalid magic hits are
/// ignored, attempt and result counts are capped independently, and strict
/// exact-end validation is applied to every reported prefix.
pub fn scan_recovery_candidates(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<ImmutableRecoveryReport, ImmutableError> {
    if data.len() > limits.max_file_bytes {
        return Err(ImmutableError::Limit("file size"));
    }
    let scanned_bytes = data.len().min(limits.max_recovery_scan_bytes);
    let scan_start = data.len() - scanned_bytes;
    let mut attempted_footers = 0_usize;
    let mut attempts_truncated = false;
    let mut candidates_truncated = false;
    let mut candidates = Vec::new();

    if scanned_bytes >= FOOTER_MAGIC.len() {
        let mut offset = data.len().saturating_sub(FOOTER_MAGIC.len());
        loop {
            if offset < scan_start {
                break;
            }
            if data.get(offset..offset + FOOTER_MAGIC.len()) == Some(FOOTER_MAGIC) {
                if attempted_footers >= limits.max_recovery_attempts {
                    attempts_truncated = true;
                    break;
                }
                attempted_footers += 1;
                if let Some(prefix_len) = offset.checked_add(FOOTER_LEN) {
                    if prefix_len <= data.len() {
                        if let Ok(internal) = validate_internal(&data[..prefix_len], limits) {
                            if internal.footer_offset == offset {
                                if candidates.len() >= limits.max_recovery_candidates {
                                    candidates_truncated = true;
                                    break;
                                }
                                allocation_check::<ImmutableRecoveryCandidate>(
                                    candidates.len() + 1,
                                    limits,
                                )?;
                                candidates.push(ImmutableRecoveryCandidate {
                                    footer_offset: u64_from_usize(offset)?,
                                    prefix_len: u64_from_usize(prefix_len)?,
                                    report: internal.public,
                                });
                            }
                        }
                    }
                }
            }
            if offset == scan_start {
                break;
            }
            offset -= 1;
        }
    }

    Ok(ImmutableRecoveryReport {
        scan_start: u64_from_usize(scan_start)?,
        scanned_bytes,
        attempted_footers,
        attempts_truncated,
        candidates_truncated,
        candidates,
    })
}
