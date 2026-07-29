use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryScanLimits {
    pub max_scan_bytes: usize,
    pub max_magic_candidates: usize,
    pub max_validations: usize,
    pub max_results: usize,
}

impl Default for RecoveryScanLimits {
    fn default() -> Self {
        Self {
            max_scan_bytes: 64 * 1024,
            max_magic_candidates: 128,
            max_validations: 64,
            max_results: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryScanError {
    EmptyMagic,
    CandidateShorterThanMagic,
    MagicCandidateLimitExceeded,
    ValidationLimitExceeded,
    ResultLimitExceeded,
}

impl fmt::Display for RecoveryScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMagic => write!(f, "recovery magic must not be empty"),
            Self::CandidateShorterThanMagic => {
                write!(f, "recovery candidate is shorter than its magic")
            }
            Self::MagicCandidateLimitExceeded => {
                write!(f, "recovery magic-candidate limit exceeded")
            }
            Self::ValidationLimitExceeded => {
                write!(f, "recovery candidate-validation limit exceeded")
            }
            Self::ResultLimitExceeded => write!(f, "recovery result limit exceeded"),
        }
    }
}

impl std::error::Error for RecoveryScanError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedCandidate<T> {
    pub offset: usize,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryScanReport<T> {
    /// Candidates ordered from newest physical offset to oldest.
    pub candidates: Vec<ScannedCandidate<T>>,
    pub bytes_in_search_window: usize,
    pub magic_matches: usize,
    pub validations: usize,
    /// True when bytes exist before the bounded search window.
    pub earlier_bytes_unsearched: bool,
}

/// Returns whether one complete candidate with `magic` begins exactly
/// `candidate_len` bytes before the physical end.
#[must_use]
pub fn has_exact_end_candidate(source: &[u8], magic: &[u8], candidate_len: usize) -> bool {
    if magic.is_empty() || candidate_len < magic.len() || source.len() < candidate_len {
        return false;
    }
    let offset = source.len() - candidate_len;
    source
        .get(offset..offset + magic.len())
        .is_some_and(|candidate_magic| candidate_magic == magic)
}

/// Searches one bounded tail window for complete candidate-sized regions.
///
/// A magic match receives no validity status by itself. The caller-supplied
/// validator performs cheap or full candidate validation and returns `Some`
/// only for results that should be included. All matches, validations, and
/// returned results have independent limits.
pub fn scan_backwards<T, F>(
    source: &[u8],
    magic: &[u8],
    candidate_len: usize,
    limits: RecoveryScanLimits,
    mut validate: F,
) -> Result<RecoveryScanReport<T>, RecoveryScanError>
where
    F: FnMut(usize, &[u8]) -> Option<T>,
{
    if magic.is_empty() {
        return Err(RecoveryScanError::EmptyMagic);
    }
    if candidate_len < magic.len() {
        return Err(RecoveryScanError::CandidateShorterThanMagic);
    }

    let scan_start = source.len().saturating_sub(limits.max_scan_bytes);
    let mut report = RecoveryScanReport {
        candidates: Vec::new(),
        bytes_in_search_window: source.len() - scan_start,
        magic_matches: 0,
        validations: 0,
        earlier_bytes_unsearched: scan_start > 0,
    };

    if candidate_len > source.len() || limits.max_scan_bytes == 0 {
        return Ok(report);
    }

    let last_start = source.len() - candidate_len;
    if last_start < scan_start {
        return Ok(report);
    }

    for offset in (scan_start..=last_start).rev() {
        let Some(candidate_magic) = source.get(offset..offset + magic.len()) else {
            continue;
        };
        if candidate_magic != magic {
            continue;
        }

        report.magic_matches = report
            .magic_matches
            .checked_add(1)
            .ok_or(RecoveryScanError::MagicCandidateLimitExceeded)?;
        if report.magic_matches > limits.max_magic_candidates {
            return Err(RecoveryScanError::MagicCandidateLimitExceeded);
        }

        report.validations = report
            .validations
            .checked_add(1)
            .ok_or(RecoveryScanError::ValidationLimitExceeded)?;
        if report.validations > limits.max_validations {
            return Err(RecoveryScanError::ValidationLimitExceeded);
        }

        let candidate = &source[offset..offset + candidate_len];
        if let Some(value) = validate(offset, candidate) {
            if report.candidates.len() >= limits.max_results {
                return Err(RecoveryScanError::ResultLimitExceeded);
            }
            report.candidates.push(ScannedCandidate { offset, value });
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGIC: &[u8] = b"ROOTCAND";
    const CANDIDATE_LEN: usize = 32;

    fn candidate(sequence: u64) -> Vec<u8> {
        let mut bytes = vec![0_u8; CANDIDATE_LEN];
        bytes[..MAGIC.len()].copy_from_slice(MAGIC);
        bytes[8..16].copy_from_slice(&sequence.to_le_bytes());
        let check = sequence ^ 0xa5a5_a5a5_a5a5_a5a5;
        bytes[16..24].copy_from_slice(&check.to_le_bytes());
        bytes
    }

    fn validate(_: usize, bytes: &[u8]) -> Option<u64> {
        let sequence = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
        let check = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
        (check == sequence ^ 0xa5a5_a5a5_a5a5_a5a5).then_some(sequence)
    }

    #[test]
    fn strict_exact_end_never_falls_back_to_earlier_candidate() {
        let mut source = vec![7_u8; 100];
        source.extend_from_slice(&candidate(1));
        assert!(has_exact_end_candidate(&source, MAGIC, CANDIDATE_LEN));
        source.push(0xff);
        assert!(!has_exact_end_candidate(&source, MAGIC, CANDIDATE_LEN));
    }

    #[test]
    fn every_interrupted_append_cut_recovers_the_old_candidate_within_budget() {
        let mut committed = vec![0x11_u8; 4096];
        let old_offset = committed.len();
        committed.extend_from_slice(&candidate(7));
        let append = vec![0x22_u8; 8192];

        for cut in 1..=append.len() {
            let mut interrupted = committed.clone();
            interrupted.extend_from_slice(&append[..cut]);
            assert!(!has_exact_end_candidate(&interrupted, MAGIC, CANDIDATE_LEN));
            let report = scan_backwards(
                &interrupted,
                MAGIC,
                CANDIDATE_LEN,
                RecoveryScanLimits {
                    max_scan_bytes: interrupted.len(),
                    ..RecoveryScanLimits::default()
                },
                validate,
            )
            .expect("bounded recovery");
            assert_eq!(report.candidates.len(), 1);
            assert_eq!(report.candidates[0].offset, old_offset);
            assert_eq!(report.candidates[0].value, 7);
        }
    }

    #[test]
    fn old_candidate_outside_tail_window_is_not_guessed() {
        let mut source = candidate(3);
        source.extend_from_slice(&vec![0_u8; 128 * 1024]);
        let report = scan_backwards(
            &source,
            MAGIC,
            CANDIDATE_LEN,
            RecoveryScanLimits::default(),
            validate,
        )
        .expect("bounded scan");
        assert!(report.candidates.is_empty());
        assert!(report.earlier_bytes_unsearched);
    }

    #[test]
    fn magic_candidate_storm_fails_before_unbounded_validation() {
        let mut source = Vec::new();
        for _ in 0..1024 {
            source.extend_from_slice(MAGIC);
        }
        let error = scan_backwards(
            &source,
            MAGIC,
            MAGIC.len(),
            RecoveryScanLimits {
                max_scan_bytes: source.len(),
                max_magic_candidates: 16,
                max_validations: 16,
                max_results: 16,
            },
            |_, _| Some(()),
        )
        .expect_err("candidate storm");
        assert!(matches!(
            error,
            RecoveryScanError::MagicCandidateLimitExceeded
                | RecoveryScanError::ValidationLimitExceeded
                | RecoveryScanError::ResultLimitExceeded
        ));
    }

    #[test]
    fn invalid_magic_matches_do_not_become_results() {
        let mut source = candidate(1);
        source[16] ^= 1;
        let report = scan_backwards(
            &source,
            MAGIC,
            CANDIDATE_LEN,
            RecoveryScanLimits::default(),
            validate,
        )
        .expect("scan");
        assert_eq!(report.magic_matches, 1);
        assert_eq!(report.validations, 1);
        assert!(report.candidates.is_empty());
    }
}
