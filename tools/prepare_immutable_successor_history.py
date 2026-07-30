#!/usr/bin/env python3
"""Add bounded history and recovery APIs to the reusable successor experiment."""

from pathlib import Path

path = Path("crates/ucof-experiments/src/immutable_successor.rs")
text = path.read_text(encoding="utf-8")

limits_fields = """    pub max_allocation_bytes: usize,
    pub max_output_bytes: usize,
}"""
limits_fields_replacement = """    pub max_allocation_bytes: usize,
    pub max_output_bytes: usize,
    pub max_history_entries: usize,
    pub max_recovery_scan_bytes: usize,
    pub max_recovery_attempts: usize,
    pub max_recovery_candidates: usize,
}"""
if limits_fields not in text:
    raise SystemExit("limits field insertion point not found")
text = text.replace(limits_fields, limits_fields_replacement, 1)

limits_defaults = """            max_allocation_bytes: 128 * 1024 * 1024,
            max_output_bytes: 512 * 1024 * 1024,
        }"""
limits_defaults_replacement = """            max_allocation_bytes: 128 * 1024 * 1024,
            max_output_bytes: 512 * 1024 * 1024,
            max_history_entries: 1_024,
            max_recovery_scan_bytes: 4 * 1024 * 1024,
            max_recovery_attempts: 4_096,
            max_recovery_candidates: 64,
        }"""
if limits_defaults not in text:
    raise SystemExit("limits default insertion point not found")
text = text.replace(limits_defaults, limits_defaults_replacement, 1)

type_point = """#[derive(Clone, Debug, PartialEq, Eq)]
struct Locator {"""
types = """#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableHistoryEntry {
    pub footer_offset: u64,
    pub report: ImmutableReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableHistoryReport {
    /// Strictly validated entries ordered from newest to oldest.
    pub entries: Vec<ImmutableHistoryEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableRecoveryCandidate {
    pub footer_offset: u64,
    pub prefix_len: u64,
    pub report: ImmutableReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableRecoveryReport {
    pub scan_start: u64,
    pub scanned_bytes: usize,
    pub attempted_footers: usize,
    pub attempts_truncated: bool,
    pub candidates_truncated: bool,
    /// Strictly validated prefixes ordered from newest to oldest. No candidate is selected.
    pub candidates: Vec<ImmutableRecoveryCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Locator {"""
if type_point not in text:
    raise SystemExit("public type insertion point not found")
text = text.replace(type_point, types, 1)

api_point = """pub fn validate(data: &[u8], limits: ImmutableLimits) -> Result<ImmutableReport, ImmutableError> {
    Ok(validate_internal(data, limits)?.public)
}

fn encode_object"""
apis = """pub fn validate(data: &[u8], limits: ImmutableLimits) -> Result<ImmutableReport, ImmutableError> {
    Ok(validate_internal(data, limits)?.public)
}

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

fn encode_object"""
if api_point not in text:
    raise SystemExit("API insertion point not found")
text = text.replace(api_point, apis, 1)
path.write_text(text, encoding="utf-8")
