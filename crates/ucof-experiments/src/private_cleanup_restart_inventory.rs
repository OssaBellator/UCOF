//! Test-only bounded identity inventory for cleanup restart classification.
//!
//! Absence is considered proven only after a complete bounded scan. Finding the
//! exact expected identity or a conflicting expected-name identity is decisive
//! immediately; a matching identity elsewhere is sufficient to prove that
//! private state survives under another name.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EntryIdentity([u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryMetadata {
    Known {
        identity: EntryIdentity,
        charged_bytes: u64,
    },
    Unreadable {
        charged_bytes: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InventoryEntry {
    is_expected_name: bool,
    metadata: EntryMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InventoryLimits {
    max_entries: usize,
    max_metadata_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InventoryObservation {
    ExactIdentity,
    DifferentIdentity,
    MissingNoMatchingIdentityCompleteScan,
    MissingMatchingIdentityElsewhere,
    MissingScanTruncated,
    NameMetadataUnreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InventoryReport {
    observation: InventoryObservation,
    scanned_entries: usize,
    scanned_metadata_bytes: u64,
    truncated: bool,
    unreadable_entries: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InventoryError {
    InvalidLimits,
    AccountingOverflow,
}

fn scan_restart_inventory<I>(
    entries: I,
    expected_identity: EntryIdentity,
    limits: InventoryLimits,
) -> Result<InventoryReport, InventoryError>
where
    I: IntoIterator<Item = InventoryEntry>,
{
    if limits.max_entries == 0 || limits.max_metadata_bytes == 0 {
        return Err(InventoryError::InvalidLimits);
    }

    let mut scanned_entries = 0usize;
    let mut scanned_metadata_bytes = 0u64;
    let mut truncated = false;
    let mut unreadable_entries = 0usize;
    let mut expected_name_unreadable = false;
    let mut matching_identity_elsewhere = false;

    for entry in entries {
        if scanned_entries >= limits.max_entries {
            truncated = true;
            break;
        }

        let charged_bytes = match entry.metadata {
            EntryMetadata::Known { charged_bytes, .. }
            | EntryMetadata::Unreadable { charged_bytes } => charged_bytes,
        };
        let Some(next_metadata_bytes) = scanned_metadata_bytes.checked_add(charged_bytes) else {
            truncated = true;
            break;
        };
        if next_metadata_bytes > limits.max_metadata_bytes {
            truncated = true;
            break;
        }

        scanned_entries = scanned_entries
            .checked_add(1)
            .ok_or(InventoryError::AccountingOverflow)?;
        scanned_metadata_bytes = next_metadata_bytes;

        match entry.metadata {
            EntryMetadata::Known { identity, .. } => {
                if entry.is_expected_name {
                    if identity == expected_identity {
                        return Ok(InventoryReport {
                            observation: InventoryObservation::ExactIdentity,
                            scanned_entries,
                            scanned_metadata_bytes,
                            truncated: false,
                            unreadable_entries,
                        });
                    }
                    return Ok(InventoryReport {
                        observation: InventoryObservation::DifferentIdentity,
                        scanned_entries,
                        scanned_metadata_bytes,
                        truncated: false,
                        unreadable_entries,
                    });
                }
                if identity == expected_identity {
                    matching_identity_elsewhere = true;
                }
            }
            EntryMetadata::Unreadable { .. } => {
                unreadable_entries = unreadable_entries
                    .checked_add(1)
                    .ok_or(InventoryError::AccountingOverflow)?;
                if entry.is_expected_name {
                    expected_name_unreadable = true;
                }
            }
        }
    }

    let observation = if expected_name_unreadable {
        InventoryObservation::NameMetadataUnreadable
    } else if matching_identity_elsewhere {
        InventoryObservation::MissingMatchingIdentityElsewhere
    } else if truncated || unreadable_entries != 0 {
        InventoryObservation::MissingScanTruncated
    } else {
        InventoryObservation::MissingNoMatchingIdentityCompleteScan
    };

    Ok(InventoryReport {
        observation,
        scanned_entries,
        scanned_metadata_bytes,
        truncated,
        unreadable_entries,
    })
}

pub(crate) fn classify_external_restart_inventory<I>(
    entries: I,
    expected_identity: [u8; 32],
    max_entries: usize,
    max_metadata_bytes: u64,
) -> Result<(InventoryObservation, usize, u64, bool, usize), InventoryError>
where
    I: IntoIterator<Item = (bool, Option<[u8; 32]>, u64)>,
{
    let report = scan_restart_inventory(
        entries.into_iter().map(
            |(is_expected_name, identity, charged_bytes)| InventoryEntry {
                is_expected_name,
                metadata: match identity {
                    Some(identity) => EntryMetadata::Known {
                        identity: EntryIdentity(identity),
                        charged_bytes,
                    },
                    None => EntryMetadata::Unreadable { charged_bytes },
                },
            },
        ),
        EntryIdentity(expected_identity),
        InventoryLimits {
            max_entries,
            max_metadata_bytes,
        },
    )?;
    Ok((
        report.observation,
        report.scanned_entries,
        report.scanned_metadata_bytes,
        report.truncated,
        report.unreadable_entries,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(byte: u8) -> EntryIdentity {
        EntryIdentity([byte; 32])
    }

    fn known(is_expected_name: bool, byte: u8, charged_bytes: u64) -> InventoryEntry {
        InventoryEntry {
            is_expected_name,
            metadata: EntryMetadata::Known {
                identity: identity(byte),
                charged_bytes,
            },
        }
    }

    fn limits() -> InventoryLimits {
        InventoryLimits {
            max_entries: 32,
            max_metadata_bytes: 4096,
        }
    }

    #[test]
    fn million_entry_stream_stops_at_entry_bound_and_cannot_prove_absence() {
        let entries = (0u64..1_000_000).map(|_| known(false, 2, 64));
        let report = scan_restart_inventory(entries, identity(1), limits()).expect("inventory");
        assert_eq!(report.scanned_entries, 32);
        assert_eq!(report.scanned_metadata_bytes, 2048);
        assert!(report.truncated);
        assert_eq!(
            report.observation,
            InventoryObservation::MissingScanTruncated
        );
    }

    #[test]
    fn complete_scan_without_expected_or_matching_identity_proves_absence() {
        let entries = [known(false, 2, 64), known(false, 3, 64)];
        let report = scan_restart_inventory(entries, identity(1), limits()).expect("inventory");
        assert!(!report.truncated);
        assert_eq!(report.unreadable_entries, 0);
        assert_eq!(
            report.observation,
            InventoryObservation::MissingNoMatchingIdentityCompleteScan
        );
    }

    #[test]
    fn matching_identity_elsewhere_proves_private_state_survives_under_another_name() {
        let entries = [
            known(false, 2, 64),
            known(false, 1, 64),
            known(false, 3, 64),
        ];
        let report = scan_restart_inventory(entries, identity(1), limits()).expect("inventory");
        assert_eq!(
            report.observation,
            InventoryObservation::MissingMatchingIdentityElsewhere
        );
    }

    #[test]
    fn exact_expected_name_identity_is_decisive_without_scanning_the_rest() {
        let entries = std::iter::once(known(true, 1, 64))
            .chain((0u64..1_000_000).map(|_| known(false, 2, 64)));
        let report = scan_restart_inventory(entries, identity(1), limits()).expect("inventory");
        assert_eq!(report.scanned_entries, 1);
        assert_eq!(report.observation, InventoryObservation::ExactIdentity);
    }

    #[test]
    fn conflicting_expected_name_identity_is_decisive_and_fail_closed() {
        let entries = [known(true, 9, 64), known(false, 1, 64)];
        let report = scan_restart_inventory(entries, identity(1), limits()).expect("inventory");
        assert_eq!(report.scanned_entries, 1);
        assert_eq!(report.observation, InventoryObservation::DifferentIdentity);
    }

    #[test]
    fn metadata_byte_bound_prevents_false_absence_proof() {
        let mut bounded = limits();
        bounded.max_metadata_bytes = 100;
        let entries = [known(false, 2, 64), known(false, 3, 64)];
        let report = scan_restart_inventory(entries, identity(1), bounded).expect("inventory");
        assert_eq!(report.scanned_entries, 1);
        assert_eq!(report.scanned_metadata_bytes, 64);
        assert!(report.truncated);
        assert_eq!(
            report.observation,
            InventoryObservation::MissingScanTruncated
        );
    }

    #[test]
    fn unreadable_non_expected_entry_prevents_false_absence_proof() {
        let entries = [
            known(false, 2, 64),
            InventoryEntry {
                is_expected_name: false,
                metadata: EntryMetadata::Unreadable { charged_bytes: 64 },
            },
        ];
        let report = scan_restart_inventory(entries, identity(1), limits()).expect("inventory");
        assert_eq!(report.unreadable_entries, 1);
        assert_eq!(
            report.observation,
            InventoryObservation::MissingScanTruncated
        );
    }

    #[test]
    fn unreadable_expected_name_is_explicitly_indeterminate() {
        let entries = [InventoryEntry {
            is_expected_name: true,
            metadata: EntryMetadata::Unreadable { charged_bytes: 64 },
        }];
        let report = scan_restart_inventory(entries, identity(1), limits()).expect("inventory");
        assert_eq!(
            report.observation,
            InventoryObservation::NameMetadataUnreadable
        );
    }

    #[test]
    fn matching_identity_found_before_later_truncation_remains_positive_evidence() {
        let mut bounded = limits();
        bounded.max_entries = 2;
        let entries = [
            known(false, 1, 64),
            known(false, 2, 64),
            known(false, 3, 64),
        ];
        let report = scan_restart_inventory(entries, identity(1), bounded).expect("inventory");
        assert!(report.truncated);
        assert_eq!(
            report.observation,
            InventoryObservation::MissingMatchingIdentityElsewhere
        );
    }

    #[test]
    fn accounting_overflow_or_zero_limits_fail_closed() {
        let mut invalid = limits();
        invalid.max_entries = 0;
        assert_eq!(
            scan_restart_inventory(std::iter::empty(), identity(1), invalid)
                .expect_err("invalid limits"),
            InventoryError::InvalidLimits
        );

        let entries = [known(false, 2, u64::MAX), known(false, 3, 1)];
        let mut huge = limits();
        huge.max_metadata_bytes = u64::MAX;
        let report =
            scan_restart_inventory(entries, identity(1), huge).expect("overflow truncates");
        assert_eq!(report.scanned_entries, 1);
        assert!(report.truncated);
        assert_eq!(
            report.observation,
            InventoryObservation::MissingScanTruncated
        );
    }
}
