//! Explicit bounded recovery for the provisional EXP-0002 byte candidate.
//!
//! Recovery validates candidate prefixes with the exact-end strict validator.
//! Footer magic and previous pointers are discovery aids only.

use crate::exp0002::{
    validate_strict, Exp0002Error, ValidationLimits, VerifiedExp0002, ABSENT_OFFSET, FOOTER_LEN,
};
use std::collections::BTreeSet;

const FOOTER_MAGIC: &[u8; 8] = b"UCOF2END";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002RecoveryLimits {
    pub max_scan_bytes: usize,
    pub max_magic_matches: usize,
    pub max_candidate_validations: usize,
    pub max_results: usize,
    pub max_chain_depth: usize,
}

impl Default for Exp0002RecoveryLimits {
    fn default() -> Self {
        Self {
            max_scan_bytes: 4 * 1024 * 1024,
            max_magic_matches: 4096,
            max_candidate_validations: 256,
            max_results: 64,
            max_chain_depth: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002RecoveredCandidate {
    pub footer_offset: u64,
    pub prefix_len: usize,
    pub sequence: u64,
    pub snapshot_digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002RecoveryReport {
    pub bytes_scanned: usize,
    pub magic_matches: usize,
    pub candidates_validated: usize,
    pub candidates: Vec<Exp0002RecoveredCandidate>,
}

impl Exp0002RecoveryReport {
    pub fn latest(&self) -> Option<&Exp0002RecoveredCandidate> {
        self.candidates.first()
    }
}

pub fn scan_valid_prefixes(
    bytes: &[u8],
    validation_limits: &ValidationLimits,
    recovery_limits: &Exp0002RecoveryLimits,
) -> Result<Exp0002RecoveryReport, Exp0002Error> {
    let scan_start = bytes.len().saturating_sub(recovery_limits.max_scan_bytes);
    let bytes_scanned = bytes.len() - scan_start;
    let mut magic_matches = 0_usize;
    let mut candidates_validated = 0_usize;
    let mut candidates = Vec::new();

    if bytes.len() >= FOOTER_MAGIC.len() {
        for offset in (scan_start..=bytes.len() - FOOTER_MAGIC.len()).rev() {
            if &bytes[offset..offset + FOOTER_MAGIC.len()] != FOOTER_MAGIC {
                continue;
            }
            magic_matches = magic_matches
                .checked_add(1)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            if magic_matches > recovery_limits.max_magic_matches {
                return Err(Exp0002Error::ResourceLimit("recovery magic matches"));
            }
            let prefix_len = offset
                .checked_add(FOOTER_LEN)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            if prefix_len > bytes.len() {
                continue;
            }
            candidates_validated = candidates_validated
                .checked_add(1)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            if candidates_validated > recovery_limits.max_candidate_validations {
                return Err(Exp0002Error::ResourceLimit(
                    "recovery candidate validations",
                ));
            }
            if let Ok(verified) = validate_strict(&bytes[..prefix_len], validation_limits) {
                candidates.push(candidate_from_verified(prefix_len, &verified));
                if candidates.len() > recovery_limits.max_results {
                    return Err(Exp0002Error::ResourceLimit("recovery results"));
                }
            }
        }
    }

    candidates.sort_by(|left, right| {
        right
            .sequence
            .cmp(&left.sequence)
            .then_with(|| right.footer_offset.cmp(&left.footer_offset))
    });
    Ok(Exp0002RecoveryReport {
        bytes_scanned,
        magic_matches,
        candidates_validated,
        candidates,
    })
}

pub fn enumerate_previous_chain(
    bytes: &[u8],
    exact_end: &VerifiedExp0002,
    validation_limits: &ValidationLimits,
    recovery_limits: &Exp0002RecoveryLimits,
) -> Result<Vec<Exp0002RecoveredCandidate>, Exp0002Error> {
    let mut chain = Vec::new();
    let mut current = candidate_from_verified(bytes.len(), exact_end);
    let mut previous_offset = exact_end.footer.previous_footer_offset;
    let mut seen = BTreeSet::new();
    seen.insert(exact_end.footer_offset);
    chain.push(current.clone());

    while previous_offset != ABSENT_OFFSET {
        if chain.len() >= recovery_limits.max_chain_depth {
            return Err(Exp0002Error::ResourceLimit("recovery chain depth"));
        }
        if !seen.insert(previous_offset) {
            return Err(Exp0002Error::InvalidPreviousFooter);
        }
        let offset = usize::try_from(previous_offset)
            .map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let prefix_len = offset
            .checked_add(FOOTER_LEN)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if prefix_len > bytes.len() || previous_offset >= current.footer_offset {
            return Err(Exp0002Error::InvalidPreviousFooter);
        }
        let verified = validate_strict(&bytes[..prefix_len], validation_limits)?;
        if verified.footer.snapshot_digest != current_parent_digest(bytes, &current)?
            || verified
                .footer
                .sequence
                .checked_add(1)
                .ok_or(Exp0002Error::ArithmeticOverflow)?
                != current.sequence
        {
            return Err(Exp0002Error::InvalidParent);
        }
        previous_offset = verified.footer.previous_footer_offset;
        current = candidate_from_verified(prefix_len, &verified);
        chain.push(current.clone());
    }
    Ok(chain)
}

fn current_parent_digest(
    bytes: &[u8],
    current: &Exp0002RecoveredCandidate,
) -> Result<[u8; 32], Exp0002Error> {
    let verified = validate_strict(
        &bytes[..current.prefix_len],
        &ValidationLimits {
            max_file_bytes: current.prefix_len as u64,
            ..ValidationLimits::default()
        },
    )?;
    Ok(verified.snapshot.parent_snapshot_digest)
}

fn candidate_from_verified(
    prefix_len: usize,
    verified: &VerifiedExp0002,
) -> Exp0002RecoveredCandidate {
    Exp0002RecoveredCandidate {
        footer_offset: verified.footer_offset,
        prefix_len,
        sequence: verified.footer.sequence,
        snapshot_digest: verified.footer.snapshot_digest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exp0002::{build_append, build_genesis, FileHeader, ObjectInput};

    fn header() -> FileHeader {
        FileHeader {
            file_id: *b"exp0002-file-id!",
            creation_nonce: *b"fixed-nonce-0002",
        }
    }

    fn object(id: u64, payload: &[u8], root: bool) -> ObjectInput {
        ObjectInput {
            object_id: id,
            kind: 1,
            payload: payload.to_vec(),
            is_root: root,
        }
    }

    #[test]
    fn every_interrupted_append_cut_recovers_genesis() {
        let limits = ValidationLimits::default();
        let recovery = Exp0002RecoveryLimits {
            max_scan_bytes: 256 * 1024,
            ..Exp0002RecoveryLimits::default()
        };
        let genesis = build_genesis(header(), vec![object(1, b"one", true)]).expect("genesis");
        let appended = build_append(
            &genesis,
            vec![object(2, b"two", false)],
            vec![1],
            &limits,
        )
        .expect("append");
        let genesis_footer = genesis.len() as u64 - FOOTER_LEN as u64;
        for cut in genesis.len() + 1..appended.len() {
            let report = scan_valid_prefixes(&appended[..cut], &limits, &recovery).expect("scan");
            assert_eq!(report.latest().map(|value| value.footer_offset), Some(genesis_footer));
        }
    }

    #[test]
    fn complete_append_enumerates_newest_then_genesis() {
        let limits = ValidationLimits::default();
        let genesis = build_genesis(header(), vec![object(1, b"one", true)]).expect("genesis");
        let appended = build_append(
            &genesis,
            vec![object(2, b"two", false)],
            vec![1, 2],
            &limits,
        )
        .expect("append");
        let latest = validate_strict(&appended, &limits).expect("latest");
        let chain = enumerate_previous_chain(
            &appended,
            &latest,
            &limits,
            &Exp0002RecoveryLimits::default(),
        )
        .expect("chain");
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].sequence, 1);
        assert_eq!(chain[1].sequence, 0);
    }

    #[test]
    fn magic_candidate_storm_is_bounded() {
        let bytes = FOOTER_MAGIC.repeat(100);
        assert_eq!(
            scan_valid_prefixes(
                &bytes,
                &ValidationLimits::default(),
                &Exp0002RecoveryLimits {
                    max_scan_bytes: bytes.len(),
                    max_magic_matches: 4,
                    ..Exp0002RecoveryLimits::default()
                }
            ),
            Err(Exp0002Error::ResourceLimit("recovery magic matches"))
        );
    }

    #[test]
    fn scan_window_does_not_guess_outside_bytes() {
        let limits = ValidationLimits::default();
        let genesis = build_genesis(header(), vec![object(1, b"one", true)]).expect("genesis");
        let mut damaged = genesis.clone();
        damaged.extend(vec![0_u8; 4096]);
        let report = scan_valid_prefixes(
            &damaged,
            &limits,
            &Exp0002RecoveryLimits {
                max_scan_bytes: 1024,
                ..Exp0002RecoveryLimits::default()
            },
        )
        .expect("scan");
        assert!(report.candidates.is_empty());
    }
}
