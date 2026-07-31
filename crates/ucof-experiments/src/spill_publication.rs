use std::error::Error;
use std::fmt;

/// Caller-selected confidentiality requirement for one spill operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillConfidentialityPolicy {
    PlaintextPermitted,
    EncryptedSpillRequired,
}

/// Externally meaningful publication result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillPublicationOutcome {
    NotPublished,
    PublishedAndDurable,
    PublicationIndeterminate,
}

/// Internal progress for one no-overwrite same-filesystem publication attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillPublicationStage {
    PrivateStaging,
    OutputValidated,
    StagedFileSynchronized,
    DestinationLinked,
    DestinationDirectorySynchronized,
    PrivateNameRetired,
}

/// Result reported by the platform-specific no-overwrite link or rename primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoOverwriteLinkResult {
    DestinationExists,
    NotCreated,
    Created,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpillPublicationLimits {
    pub max_staged_bytes: u64,
    pub max_staged_files: usize,
    pub max_cleanup_actions: usize,
}

impl Default for SpillPublicationLimits {
    fn default() -> Self {
        Self {
            max_staged_bytes: 8 * 1024 * 1024 * 1024,
            max_staged_files: 65_536,
            max_cleanup_actions: 65_536,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpillPublicationError {
    InvalidOwnershipToken,
    OwnershipMismatch,
    InvalidTransition,
    Limit(&'static str),
    DestinationExists,
    NotPublished(&'static str),
    PublicationIndeterminate(&'static str),
    CleanupFailed(&'static str),
}

impl fmt::Display for SpillPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOwnershipToken => write!(formatter, "invalid spill ownership token"),
            Self::OwnershipMismatch => write!(formatter, "spill ownership token mismatch"),
            Self::InvalidTransition => write!(formatter, "invalid spill publication transition"),
            Self::Limit(label) => write!(formatter, "spill {label} limit exceeded"),
            Self::DestinationExists => write!(formatter, "destination already exists"),
            Self::NotPublished(label) => write!(formatter, "output was not published: {label}"),
            Self::PublicationIndeterminate(label) => {
                write!(formatter, "publication is indeterminate: {label}")
            }
            Self::CleanupFailed(label) => write!(formatter, "spill cleanup failed: {label}"),
        }
    }
}

impl Error for SpillPublicationError {}

/// Pure policy state machine for private staging, ownership-checked cleanup, and no-overwrite
/// publication reporting.
///
/// Platform adapters remain responsible for secure directory handles, exclusive/no-follow opens,
/// object-type and link-count checks, actual writes, synchronization, and qualified filesystem
/// semantics. This model prevents those adapters from collapsing an indeterminate publication into
/// an ordinary failure or allowing cleanup to downgrade durable success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpillPublicationSession {
    ownership_token: [u8; 32],
    confidentiality: SpillConfidentialityPolicy,
    limits: SpillPublicationLimits,
    stage: SpillPublicationStage,
    outcome: SpillPublicationOutcome,
    staged_bytes: u64,
    staged_files: usize,
    cleanup_actions: usize,
}

impl SpillPublicationSession {
    pub fn new(
        ownership_token: [u8; 32],
        confidentiality: SpillConfidentialityPolicy,
        limits: SpillPublicationLimits,
    ) -> Result<Self, SpillPublicationError> {
        if ownership_token.iter().all(|byte| *byte == 0) {
            return Err(SpillPublicationError::InvalidOwnershipToken);
        }
        Ok(Self {
            ownership_token,
            confidentiality,
            limits,
            stage: SpillPublicationStage::PrivateStaging,
            outcome: SpillPublicationOutcome::NotPublished,
            staged_bytes: 0,
            staged_files: 0,
            cleanup_actions: 0,
        })
    }

    #[must_use]
    pub fn confidentiality(&self) -> SpillConfidentialityPolicy {
        self.confidentiality
    }

    #[must_use]
    pub fn stage(&self) -> SpillPublicationStage {
        self.stage
    }

    #[must_use]
    pub fn outcome(&self) -> SpillPublicationOutcome {
        self.outcome
    }

    #[must_use]
    pub fn staged_bytes(&self) -> u64 {
        self.staged_bytes
    }

    #[must_use]
    pub fn staged_files(&self) -> usize {
        self.staged_files
    }

    #[must_use]
    pub fn cleanup_actions(&self) -> usize {
        self.cleanup_actions
    }

    fn check_owner(&self, token: &[u8; 32]) -> Result<(), SpillPublicationError> {
        if token != &self.ownership_token {
            return Err(SpillPublicationError::OwnershipMismatch);
        }
        Ok(())
    }

    pub fn record_staged_file(
        &mut self,
        token: &[u8; 32],
        bytes: u64,
    ) -> Result<(), SpillPublicationError> {
        self.check_owner(token)?;
        if self.stage != SpillPublicationStage::PrivateStaging {
            return Err(SpillPublicationError::InvalidTransition);
        }
        let next_files = self
            .staged_files
            .checked_add(1)
            .ok_or(SpillPublicationError::Limit("file count"))?;
        let next_bytes = self
            .staged_bytes
            .checked_add(bytes)
            .ok_or(SpillPublicationError::Limit("byte count"))?;
        if next_files > self.limits.max_staged_files {
            return Err(SpillPublicationError::Limit("file count"));
        }
        if next_bytes > self.limits.max_staged_bytes {
            return Err(SpillPublicationError::Limit("byte count"));
        }
        self.staged_files = next_files;
        self.staged_bytes = next_bytes;
        Ok(())
    }

    pub fn record_complete_validation(
        &mut self,
        token: &[u8; 32],
    ) -> Result<(), SpillPublicationError> {
        self.check_owner(token)?;
        if self.stage != SpillPublicationStage::PrivateStaging || self.staged_files == 0 {
            return Err(SpillPublicationError::InvalidTransition);
        }
        self.stage = SpillPublicationStage::OutputValidated;
        Ok(())
    }

    pub fn record_staged_file_sync(
        &mut self,
        token: &[u8; 32],
        succeeded: bool,
    ) -> Result<(), SpillPublicationError> {
        self.check_owner(token)?;
        if self.stage != SpillPublicationStage::OutputValidated {
            return Err(SpillPublicationError::InvalidTransition);
        }
        if !succeeded {
            return Err(SpillPublicationError::NotPublished(
                "staged file synchronization",
            ));
        }
        self.stage = SpillPublicationStage::StagedFileSynchronized;
        Ok(())
    }

    pub fn record_no_overwrite_link(
        &mut self,
        token: &[u8; 32],
        result: NoOverwriteLinkResult,
    ) -> Result<(), SpillPublicationError> {
        self.check_owner(token)?;
        if self.stage != SpillPublicationStage::StagedFileSynchronized {
            return Err(SpillPublicationError::InvalidTransition);
        }
        match result {
            NoOverwriteLinkResult::DestinationExists => {
                Err(SpillPublicationError::DestinationExists)
            }
            NoOverwriteLinkResult::NotCreated => {
                Err(SpillPublicationError::NotPublished("destination link"))
            }
            NoOverwriteLinkResult::Created => {
                self.stage = SpillPublicationStage::DestinationLinked;
                Ok(())
            }
            NoOverwriteLinkResult::Indeterminate => {
                self.outcome = SpillPublicationOutcome::PublicationIndeterminate;
                Err(SpillPublicationError::PublicationIndeterminate(
                    "destination link",
                ))
            }
        }
    }

    pub fn record_destination_directory_sync(
        &mut self,
        token: &[u8; 32],
        succeeded: bool,
    ) -> Result<(), SpillPublicationError> {
        self.check_owner(token)?;
        if self.stage != SpillPublicationStage::DestinationLinked {
            return Err(SpillPublicationError::InvalidTransition);
        }
        if !succeeded {
            self.outcome = SpillPublicationOutcome::PublicationIndeterminate;
            return Err(SpillPublicationError::PublicationIndeterminate(
                "destination directory synchronization",
            ));
        }
        self.stage = SpillPublicationStage::DestinationDirectorySynchronized;
        self.outcome = SpillPublicationOutcome::PublishedAndDurable;
        Ok(())
    }

    pub fn record_private_name_retirement(
        &mut self,
        token: &[u8; 32],
        succeeded: bool,
    ) -> Result<(), SpillPublicationError> {
        self.check_owner(token)?;
        if self.stage != SpillPublicationStage::DestinationDirectorySynchronized {
            return Err(SpillPublicationError::InvalidTransition);
        }
        self.record_cleanup_action()?;
        if !succeeded {
            return Err(SpillPublicationError::CleanupFailed(
                "private staged name retirement",
            ));
        }
        self.stage = SpillPublicationStage::PrivateNameRetired;
        Ok(())
    }

    pub fn record_owned_cleanup(
        &mut self,
        token: &[u8; 32],
        succeeded: bool,
    ) -> Result<(), SpillPublicationError> {
        self.check_owner(token)?;
        self.record_cleanup_action()?;
        if !succeeded {
            return Err(SpillPublicationError::CleanupFailed("owned artifact"));
        }
        Ok(())
    }

    fn record_cleanup_action(&mut self) -> Result<(), SpillPublicationError> {
        let next = self
            .cleanup_actions
            .checked_add(1)
            .ok_or(SpillPublicationError::Limit("cleanup work"))?;
        if next > self.limits.max_cleanup_actions {
            return Err(SpillPublicationError::Limit("cleanup work"));
        }
        self.cleanup_actions = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> [u8; 32] {
        [7_u8; 32]
    }

    fn ready_session() -> SpillPublicationSession {
        let ownership = token();
        let mut session = SpillPublicationSession::new(
            ownership,
            SpillConfidentialityPolicy::PlaintextPermitted,
            SpillPublicationLimits::default(),
        )
        .expect("session");
        session
            .record_staged_file(&ownership, 4096)
            .expect("stage file");
        session
            .record_complete_validation(&ownership)
            .expect("validation");
        session
            .record_staged_file_sync(&ownership, true)
            .expect("file sync");
        session
    }

    #[test]
    fn budgets_and_ownership_fail_before_mutation() {
        let ownership = token();
        let limits = SpillPublicationLimits {
            max_staged_bytes: 10,
            max_staged_files: 1,
            max_cleanup_actions: 1,
        };
        let mut session = SpillPublicationSession::new(
            ownership,
            SpillConfidentialityPolicy::EncryptedSpillRequired,
            limits,
        )
        .expect("session");
        assert_eq!(
            session.record_staged_file(&[8_u8; 32], 1),
            Err(SpillPublicationError::OwnershipMismatch)
        );
        assert_eq!(session.staged_bytes(), 0);
        session
            .record_staged_file(&ownership, 10)
            .expect("within budget");
        assert_eq!(
            session.record_staged_file(&ownership, 1),
            Err(SpillPublicationError::Limit("file count"))
        );
        assert_eq!(session.staged_bytes(), 10);
        assert_eq!(
            session.confidentiality(),
            SpillConfidentialityPolicy::EncryptedSpillRequired
        );
    }

    #[test]
    fn destination_exists_is_not_reported_as_publication() {
        let ownership = token();
        let mut session = ready_session();
        assert_eq!(
            session.record_no_overwrite_link(&ownership, NoOverwriteLinkResult::DestinationExists,),
            Err(SpillPublicationError::DestinationExists)
        );
        assert_eq!(session.outcome(), SpillPublicationOutcome::NotPublished);
        assert_eq!(
            session.stage(),
            SpillPublicationStage::StagedFileSynchronized
        );
    }

    #[test]
    fn post_link_sync_failure_is_indeterminate() {
        let ownership = token();
        let mut session = ready_session();
        session
            .record_no_overwrite_link(&ownership, NoOverwriteLinkResult::Created)
            .expect("link");
        assert_eq!(
            session.record_destination_directory_sync(&ownership, false),
            Err(SpillPublicationError::PublicationIndeterminate(
                "destination directory synchronization"
            ))
        );
        assert_eq!(
            session.outcome(),
            SpillPublicationOutcome::PublicationIndeterminate
        );
    }

    #[test]
    fn cleanup_failure_does_not_downgrade_durable_publication() {
        let ownership = token();
        let mut session = ready_session();
        session
            .record_no_overwrite_link(&ownership, NoOverwriteLinkResult::Created)
            .expect("link");
        session
            .record_destination_directory_sync(&ownership, true)
            .expect("directory sync");
        assert_eq!(
            session.record_private_name_retirement(&ownership, false),
            Err(SpillPublicationError::CleanupFailed(
                "private staged name retirement"
            ))
        );
        assert_eq!(
            session.outcome(),
            SpillPublicationOutcome::PublishedAndDurable
        );
        assert_eq!(session.cleanup_actions(), 1);
    }

    #[test]
    fn indeterminate_link_is_never_collapsed_to_not_published() {
        let ownership = token();
        let mut session = ready_session();
        assert_eq!(
            session.record_no_overwrite_link(&ownership, NoOverwriteLinkResult::Indeterminate,),
            Err(SpillPublicationError::PublicationIndeterminate(
                "destination link"
            ))
        );
        assert_eq!(
            session.outcome(),
            SpillPublicationOutcome::PublicationIndeterminate
        );
    }
}
