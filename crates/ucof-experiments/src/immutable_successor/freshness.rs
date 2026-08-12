/// Trusted application state kept outside the UCOF file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrustedFreshnessCheckpoint {
    pub sequence: u64,
    pub snapshot_digest: [u8; 32],
}

impl From<&ImmutableReport> for TrustedFreshnessCheckpoint {
    fn from(report: &ImmutableReport) -> Self {
        Self {
            sequence: report.sequence,
            snapshot_digest: report.snapshot_digest,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreshnessDecision {
    /// No trusted checkpoint exists. Integrity may be established, but freshness is unpinned.
    Unpinned,
    /// The candidate is exactly the trusted checkpoint.
    Current,
    /// The candidate advances beyond the trusted checkpoint and may replace it after application
    /// policy accepts the transition.
    Advances {
        previous_sequence: u64,
        candidate_sequence: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreshnessError {
    Rollback {
        trusted_sequence: u64,
        candidate_sequence: u64,
    },
    ForkAtTrustedSequence {
        sequence: u64,
        trusted_snapshot_digest: [u8; 32],
        candidate_snapshot_digest: [u8; 32],
    },
}

impl fmt::Display for FreshnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rollback {
                trusted_sequence,
                candidate_sequence,
            } => write!(
                formatter,
                "candidate sequence {candidate_sequence} is older than trusted sequence {trusted_sequence}"
            ),
            Self::ForkAtTrustedSequence { sequence, .. } => {
                write!(formatter, "candidate conflicts with trusted sequence {sequence}")
            }
        }
    }
}

impl Error for FreshnessError {}

/// Compares a strictly validated active report with trusted application state.
///
/// This function does not persist or authenticate the checkpoint. Applications requiring rollback
/// resistance must store it in a trusted external system and define when an advancing checkpoint is
/// durably accepted.
pub fn evaluate_freshness(
    candidate: &ImmutableReport,
    trusted: Option<TrustedFreshnessCheckpoint>,
) -> Result<FreshnessDecision, FreshnessError> {
    let Some(trusted) = trusted else {
        return Ok(FreshnessDecision::Unpinned);
    };

    if candidate.sequence < trusted.sequence {
        return Err(FreshnessError::Rollback {
            trusted_sequence: trusted.sequence,
            candidate_sequence: candidate.sequence,
        });
    }
    if candidate.sequence == trusted.sequence {
        if candidate.snapshot_digest != trusted.snapshot_digest {
            return Err(FreshnessError::ForkAtTrustedSequence {
                sequence: candidate.sequence,
                trusted_snapshot_digest: trusted.snapshot_digest,
                candidate_snapshot_digest: candidate.snapshot_digest,
            });
        }
        return Ok(FreshnessDecision::Current);
    }

    Ok(FreshnessDecision::Advances {
        previous_sequence: trusted.sequence,
        candidate_sequence: candidate.sequence,
    })
}
