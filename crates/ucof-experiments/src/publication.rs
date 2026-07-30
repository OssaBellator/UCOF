use crate::{CheckpointKind, SnapshotIdentity};
use std::fmt;

/// Logical append stages used to test publication ordering before bytes exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PublicationStage {
    Objects,
    DirectoryLeaves,
    DirectoryRoot,
    SnapshotManifest,
    Footer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationLimits {
    pub max_events: usize,
    pub max_complete_checkpoints: usize,
    pub max_progress_checkpoints: usize,
}

impl Default for PublicationLimits {
    fn default() -> Self {
        Self {
            max_events: 1_000_000,
            max_complete_checkpoints: 1024,
            max_progress_checkpoints: 4096,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationError {
    EventLimitExceeded,
    StageRegression,
    DuplicateStage(PublicationStage),
    FooterBeforeSnapshot,
    CheckpointBeforeObjects,
    CompleteCheckpointLimitExceeded,
    ProgressCheckpointLimitExceeded,
    AlreadyPublished,
}

impl fmt::Display for PublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLimitExceeded => write!(f, "publication event limit exceeded"),
            Self::StageRegression => write!(f, "publication stage regressed"),
            Self::DuplicateStage(stage) => write!(f, "duplicate publication stage {stage:?}"),
            Self::FooterBeforeSnapshot => write!(f, "footer attempted before snapshot manifest"),
            Self::CheckpointBeforeObjects => write!(f, "checkpoint attempted before object stage"),
            Self::CompleteCheckpointLimitExceeded => {
                write!(f, "complete checkpoint limit exceeded")
            }
            Self::ProgressCheckpointLimitExceeded => {
                write!(f, "progress checkpoint limit exceeded")
            }
            Self::AlreadyPublished => write!(f, "append already published"),
        }
    }
}

impl std::error::Error for PublicationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedCheckpoint {
    pub identity: SnapshotIdentity,
    pub sequence: u64,
    pub kind: CheckpointKind,
    pub independently_readable: bool,
    pub active_root_eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationReport {
    pub published_snapshot: Option<SnapshotIdentity>,
    pub published_sequence: Option<u64>,
    pub complete_checkpoints: Vec<PublishedCheckpoint>,
    pub progress_checkpoints: Vec<PublishedCheckpoint>,
    pub last_complete_snapshot: Option<SnapshotIdentity>,
    pub event_count: usize,
}

#[derive(Debug, Clone)]
pub struct PublicationModel {
    identity: SnapshotIdentity,
    sequence: u64,
    limits: PublicationLimits,
    current_stage: Option<PublicationStage>,
    stage_seen: [bool; 5],
    published: bool,
    complete_checkpoints: Vec<PublishedCheckpoint>,
    progress_checkpoints: Vec<PublishedCheckpoint>,
    last_complete_snapshot: Option<SnapshotIdentity>,
    event_count: usize,
}

impl PublicationModel {
    #[must_use]
    pub fn new(
        identity: SnapshotIdentity,
        sequence: u64,
        previous_complete: Option<SnapshotIdentity>,
        limits: PublicationLimits,
    ) -> Self {
        Self {
            identity,
            sequence,
            limits,
            current_stage: None,
            stage_seen: [false; 5],
            published: false,
            complete_checkpoints: Vec::new(),
            progress_checkpoints: Vec::new(),
            last_complete_snapshot: previous_complete,
            event_count: 0,
        }
    }

    pub fn advance(&mut self, stage: PublicationStage) -> Result<(), PublicationError> {
        self.record_event()?;
        if self.published {
            return Err(PublicationError::AlreadyPublished);
        }
        if let Some(current) = self.current_stage {
            if stage < current {
                return Err(PublicationError::StageRegression);
            }
        }
        let index = stage_index(stage);
        if self.stage_seen[index] {
            return Err(PublicationError::DuplicateStage(stage));
        }
        if matches!(stage, PublicationStage::Footer)
            && !self.stage_seen[stage_index(PublicationStage::SnapshotManifest)]
        {
            return Err(PublicationError::FooterBeforeSnapshot);
        }
        self.stage_seen[index] = true;
        self.current_stage = Some(stage);
        if matches!(stage, PublicationStage::Footer) {
            self.published = true;
            self.last_complete_snapshot = Some(self.identity);
        }
        Ok(())
    }

    pub fn checkpoint(
        &mut self,
        identity: SnapshotIdentity,
        sequence: u64,
        kind: CheckpointKind,
    ) -> Result<PublishedCheckpoint, PublicationError> {
        self.record_event()?;
        if self.published {
            return Err(PublicationError::AlreadyPublished);
        }
        if !self.stage_seen[stage_index(PublicationStage::Objects)] {
            return Err(PublicationError::CheckpointBeforeObjects);
        }

        let checkpoint = match kind {
            CheckpointKind::Complete => {
                if self.complete_checkpoints.len() >= self.limits.max_complete_checkpoints {
                    return Err(PublicationError::CompleteCheckpointLimitExceeded);
                }
                let checkpoint = PublishedCheckpoint {
                    identity,
                    sequence,
                    kind,
                    independently_readable: true,
                    active_root_eligible: true,
                };
                self.complete_checkpoints.push(checkpoint);
                self.last_complete_snapshot = Some(identity);
                checkpoint
            }
            CheckpointKind::Progress => {
                if self.progress_checkpoints.len() >= self.limits.max_progress_checkpoints {
                    return Err(PublicationError::ProgressCheckpointLimitExceeded);
                }
                let checkpoint = PublishedCheckpoint {
                    identity,
                    sequence,
                    kind,
                    independently_readable: false,
                    active_root_eligible: false,
                };
                self.progress_checkpoints.push(checkpoint);
                checkpoint
            }
        };
        Ok(checkpoint)
    }

    #[must_use]
    pub fn report(&self) -> PublicationReport {
        PublicationReport {
            published_snapshot: self.published.then_some(self.identity),
            published_sequence: self.published.then_some(self.sequence),
            complete_checkpoints: self.complete_checkpoints.clone(),
            progress_checkpoints: self.progress_checkpoints.clone(),
            last_complete_snapshot: self.last_complete_snapshot,
            event_count: self.event_count,
        }
    }

    fn record_event(&mut self) -> Result<(), PublicationError> {
        if self.event_count >= self.limits.max_events {
            return Err(PublicationError::EventLimitExceeded);
        }
        self.event_count += 1;
        Ok(())
    }
}

const fn stage_index(stage: PublicationStage) -> usize {
    match stage {
        PublicationStage::Objects => 0,
        PublicationStage::DirectoryLeaves => 1,
        PublicationStage::DirectoryRoot => 2,
        PublicationStage::SnapshotManifest => 3,
        PublicationStage::Footer => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(label: &str) -> SnapshotIdentity {
        SnapshotIdentity::derive(label.as_bytes())
    }

    #[test]
    fn only_footer_publishes_main_snapshot() {
        let mut model =
            PublicationModel::new(id("new"), 2, Some(id("old")), PublicationLimits::default());
        for stage in [
            PublicationStage::Objects,
            PublicationStage::DirectoryLeaves,
            PublicationStage::DirectoryRoot,
            PublicationStage::SnapshotManifest,
        ] {
            model.advance(stage).expect("stage");
            let report = model.report();
            assert_eq!(report.published_snapshot, None);
            assert_eq!(report.last_complete_snapshot, Some(id("old")));
        }
        model.advance(PublicationStage::Footer).expect("footer");
        let report = model.report();
        assert_eq!(report.published_snapshot, Some(id("new")));
        assert_eq!(report.last_complete_snapshot, Some(id("new")));
    }

    #[test]
    fn every_interrupted_stage_preserves_previous_complete_snapshot() {
        let stages = [
            PublicationStage::Objects,
            PublicationStage::DirectoryLeaves,
            PublicationStage::DirectoryRoot,
            PublicationStage::SnapshotManifest,
        ];
        for cut in 0..=stages.len() {
            let mut model =
                PublicationModel::new(id("new"), 2, Some(id("old")), PublicationLimits::default());
            for &stage in &stages[..cut] {
                model.advance(stage).expect("stage before interruption");
            }
            let report = model.report();
            assert_eq!(report.published_snapshot, None);
            assert_eq!(report.last_complete_snapshot, Some(id("old")));
        }
    }

    #[test]
    fn complete_and_progress_checkpoints_have_distinct_authority() {
        let mut model =
            PublicationModel::new(id("new"), 3, Some(id("old")), PublicationLimits::default());
        model.advance(PublicationStage::Objects).expect("objects");
        let progress = model
            .checkpoint(id("progress"), 2, CheckpointKind::Progress)
            .expect("progress checkpoint");
        assert!(!progress.independently_readable);
        assert!(!progress.active_root_eligible);
        assert_eq!(model.report().last_complete_snapshot, Some(id("old")));

        let complete = model
            .checkpoint(id("complete"), 2, CheckpointKind::Complete)
            .expect("complete checkpoint");
        assert!(complete.independently_readable);
        assert!(complete.active_root_eligible);
        assert_eq!(model.report().last_complete_snapshot, Some(id("complete")));
    }

    #[test]
    fn footer_before_snapshot_is_rejected() {
        let mut model = PublicationModel::new(id("new"), 1, None, PublicationLimits::default());
        model.advance(PublicationStage::Objects).expect("objects");
        let error = model
            .advance(PublicationStage::Footer)
            .expect_err("early footer");
        assert_eq!(error, PublicationError::FooterBeforeSnapshot);
    }

    #[test]
    fn stages_cannot_regress_or_repeat() {
        let mut model = PublicationModel::new(id("new"), 1, None, PublicationLimits::default());
        model.advance(PublicationStage::Objects).expect("objects");
        assert_eq!(
            model.advance(PublicationStage::Objects),
            Err(PublicationError::DuplicateStage(PublicationStage::Objects))
        );
        model
            .advance(PublicationStage::DirectoryLeaves)
            .expect("leaves");
        assert_eq!(
            model.advance(PublicationStage::Objects),
            Err(PublicationError::StageRegression)
        );
    }

    #[test]
    fn publication_event_limit_is_enforced_before_mutation() {
        let mut model = PublicationModel::new(
            id("new"),
            1,
            None,
            PublicationLimits {
                max_events: 1,
                ..PublicationLimits::default()
            },
        );
        model
            .advance(PublicationStage::Objects)
            .expect("first event");
        assert_eq!(
            model.advance(PublicationStage::DirectoryLeaves),
            Err(PublicationError::EventLimitExceeded)
        );
        assert_eq!(model.report().event_count, 1);
    }
}
