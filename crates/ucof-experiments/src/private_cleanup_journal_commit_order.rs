//! Test-only crash-ordering model for authoritative private cleanup.
//!
//! The model requires cleanup intent to be durably journaled before any
//! destructive effect and requires staging-directory durability before a
//! terminal cleanup generation can become authoritative.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArtifactBinding {
    identity: [u8; 32],
    private_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupAuthority {
    ResumeOrDiscardPrivate,
    ResolvePublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupPhase {
    PrivateActive,
    CleanupPrepared(ArtifactBinding),
    TerminalDiscarded(ArtifactBinding),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CleanupJournal {
    operation_id: [u8; 16],
    generation: u64,
    authority: CleanupAuthority,
    phase: CleanupPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingPrepared {
    base: CleanupJournal,
    candidate: CleanupJournal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedExecution {
    journal: CleanupJournal,
    artifact: ArtifactBinding,
    unlink_complete: bool,
    staging_directory_synced: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingTerminal {
    base: CleanupJournal,
    candidate: CleanupJournal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartNameObservation {
    ExactIdentity,
    DifferentIdentity,
    MissingNoMatchingIdentityCompleteScan,
    MissingMatchingIdentityElsewhere,
    MissingScanTruncated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartDisposition {
    ResumePrivate,
    RetryExactCleanup,
    SyncDirectoryThenFinalize,
    ResolveRenamedPrivate,
    RetainIndeterminate,
    CleanupTerminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupJournalError {
    ResolvePublication,
    InvalidPhase,
    GenerationExhausted,
    NotDurablyCommitted,
    StaleBase,
    ArtifactMismatch,
    UnlinkIncomplete,
    DirectoryNotSynced,
}

fn active_journal() -> CleanupJournal {
    CleanupJournal {
        operation_id: [0x51; 16],
        generation: 0,
        authority: CleanupAuthority::ResumeOrDiscardPrivate,
        phase: CleanupPhase::PrivateActive,
    }
}

fn plan_cleanup_prepared(
    durable: CleanupJournal,
    artifact: ArtifactBinding,
) -> Result<PendingPrepared, CleanupJournalError> {
    if durable.authority == CleanupAuthority::ResolvePublication {
        return Err(CleanupJournalError::ResolvePublication);
    }
    if durable.phase != CleanupPhase::PrivateActive {
        return Err(CleanupJournalError::InvalidPhase);
    }
    let candidate = CleanupJournal {
        generation: durable
            .generation
            .checked_add(1)
            .ok_or(CleanupJournalError::GenerationExhausted)?,
        phase: CleanupPhase::CleanupPrepared(artifact),
        ..durable
    };
    Ok(PendingPrepared {
        base: durable,
        candidate,
    })
}

fn activate_cleanup_prepared(
    current: CleanupJournal,
    pending: PendingPrepared,
    durably_committed: bool,
) -> Result<PreparedExecution, CleanupJournalError> {
    if current != pending.base {
        return Err(CleanupJournalError::StaleBase);
    }
    if !durably_committed {
        return Err(CleanupJournalError::NotDurablyCommitted);
    }
    let CleanupPhase::CleanupPrepared(artifact) = pending.candidate.phase else {
        return Err(CleanupJournalError::InvalidPhase);
    };
    Ok(PreparedExecution {
        journal: pending.candidate,
        artifact,
        unlink_complete: false,
        staging_directory_synced: false,
    })
}

fn mark_unlink_complete(
    execution: &mut PreparedExecution,
    observed_artifact: ArtifactBinding,
) -> Result<(), CleanupJournalError> {
    if observed_artifact != execution.artifact {
        return Err(CleanupJournalError::ArtifactMismatch);
    }
    execution.unlink_complete = true;
    Ok(())
}

fn mark_staging_directory_synced(
    execution: &mut PreparedExecution,
) -> Result<(), CleanupJournalError> {
    if !execution.unlink_complete {
        return Err(CleanupJournalError::UnlinkIncomplete);
    }
    execution.staging_directory_synced = true;
    Ok(())
}

fn plan_terminal_cleanup(
    execution: PreparedExecution,
) -> Result<PendingTerminal, CleanupJournalError> {
    if !execution.unlink_complete {
        return Err(CleanupJournalError::UnlinkIncomplete);
    }
    if !execution.staging_directory_synced {
        return Err(CleanupJournalError::DirectoryNotSynced);
    }
    if execution.journal.authority == CleanupAuthority::ResolvePublication {
        return Err(CleanupJournalError::ResolvePublication);
    }
    if execution.journal.phase != CleanupPhase::CleanupPrepared(execution.artifact) {
        return Err(CleanupJournalError::InvalidPhase);
    }
    let candidate = CleanupJournal {
        generation: execution
            .journal
            .generation
            .checked_add(1)
            .ok_or(CleanupJournalError::GenerationExhausted)?,
        phase: CleanupPhase::TerminalDiscarded(execution.artifact),
        ..execution.journal
    };
    Ok(PendingTerminal {
        base: execution.journal,
        candidate,
    })
}

fn commit_terminal_cleanup(
    current: CleanupJournal,
    pending: PendingTerminal,
    durably_committed: bool,
) -> Result<CleanupJournal, CleanupJournalError> {
    if current != pending.base {
        return Err(CleanupJournalError::StaleBase);
    }
    if !durably_committed {
        return Err(CleanupJournalError::NotDurablyCommitted);
    }
    if !matches!(pending.candidate.phase, CleanupPhase::TerminalDiscarded(_)) {
        return Err(CleanupJournalError::InvalidPhase);
    }
    Ok(pending.candidate)
}

fn restart_disposition(
    durable: CleanupJournal,
    observation: RestartNameObservation,
) -> RestartDisposition {
    match durable.phase {
        CleanupPhase::PrivateActive => RestartDisposition::ResumePrivate,
        CleanupPhase::TerminalDiscarded(_) => RestartDisposition::CleanupTerminal,
        CleanupPhase::CleanupPrepared(_) => match observation {
            RestartNameObservation::ExactIdentity => RestartDisposition::RetryExactCleanup,
            RestartNameObservation::DifferentIdentity => RestartDisposition::RetainIndeterminate,
            RestartNameObservation::MissingNoMatchingIdentityCompleteScan => {
                RestartDisposition::SyncDirectoryThenFinalize
            }
            RestartNameObservation::MissingMatchingIdentityElsewhere => {
                RestartDisposition::ResolveRenamedPrivate
            }
            RestartNameObservation::MissingScanTruncated => RestartDisposition::RetainIndeterminate,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact() -> ArtifactBinding {
        ArtifactBinding {
            identity: [0x61; 32],
            private_bytes: 4096,
        }
    }

    #[test]
    fn crash_before_prepared_journal_durability_cannot_start_cleanup() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        assert_eq!(
            activate_cleanup_prepared(durable, pending, false).expect_err("not durable"),
            CleanupJournalError::NotDurablyCommitted
        );
        assert_eq!(
            restart_disposition(durable, RestartNameObservation::ExactIdentity,),
            RestartDisposition::ResumePrivate
        );
    }

    #[test]
    fn durable_prepared_generation_authorizes_exact_retry_after_crash_before_unlink() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        assert_eq!(execution.journal.generation, 1);
        assert!(!execution.unlink_complete);
        assert_eq!(
            restart_disposition(execution.journal, RestartNameObservation::ExactIdentity,),
            RestartDisposition::RetryExactCleanup
        );
    }

    #[test]
    fn terminal_generation_cannot_be_planned_before_directory_sync() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let mut execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        assert_eq!(
            plan_terminal_cleanup(execution).expect_err("unlink incomplete"),
            CleanupJournalError::UnlinkIncomplete
        );
        mark_unlink_complete(&mut execution, artifact()).expect("unlink complete");
        assert_eq!(
            plan_terminal_cleanup(execution).expect_err("directory not synced"),
            CleanupJournalError::DirectoryNotSynced
        );
        mark_staging_directory_synced(&mut execution).expect("directory sync");
        let terminal = plan_terminal_cleanup(execution).expect("plan terminal");
        assert_eq!(terminal.candidate.generation, 2);
        assert_eq!(
            terminal.candidate.phase,
            CleanupPhase::TerminalDiscarded(artifact())
        );
    }

    #[test]
    fn crash_after_unlink_before_directory_sync_remains_prepared_not_terminal() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let mut execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        mark_unlink_complete(&mut execution, artifact()).expect("unlink complete");
        assert!(!execution.staging_directory_synced);
        assert_eq!(
            restart_disposition(
                execution.journal,
                RestartNameObservation::MissingNoMatchingIdentityCompleteScan,
            ),
            RestartDisposition::SyncDirectoryThenFinalize
        );
        assert_eq!(
            execution.journal.phase,
            CleanupPhase::CleanupPrepared(artifact())
        );
    }

    #[test]
    fn crash_after_directory_sync_before_terminal_commit_can_finalize_without_retry_delete() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let mut execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        mark_unlink_complete(&mut execution, artifact()).expect("unlink complete");
        mark_staging_directory_synced(&mut execution).expect("directory sync");
        let terminal = plan_terminal_cleanup(execution).expect("plan terminal");
        assert_eq!(
            commit_terminal_cleanup(execution.journal, terminal, false)
                .expect_err("terminal not durable"),
            CleanupJournalError::NotDurablyCommitted
        );
        assert_eq!(
            restart_disposition(
                execution.journal,
                RestartNameObservation::MissingNoMatchingIdentityCompleteScan,
            ),
            RestartDisposition::SyncDirectoryThenFinalize
        );
    }

    #[test]
    fn durable_terminal_generation_is_authoritative_after_restart() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let mut execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        mark_unlink_complete(&mut execution, artifact()).expect("unlink complete");
        mark_staging_directory_synced(&mut execution).expect("directory sync");
        let terminal = plan_terminal_cleanup(execution).expect("plan terminal");
        let committed =
            commit_terminal_cleanup(execution.journal, terminal, true).expect("terminal commit");
        assert_eq!(committed.generation, 2);
        assert_eq!(
            restart_disposition(committed, RestartNameObservation::DifferentIdentity,),
            RestartDisposition::CleanupTerminal
        );
    }

    #[test]
    fn renamed_matching_identity_is_never_interpreted_as_completed_cleanup() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        assert_eq!(
            restart_disposition(
                execution.journal,
                RestartNameObservation::MissingMatchingIdentityElsewhere,
            ),
            RestartDisposition::ResolveRenamedPrivate
        );
    }

    #[test]
    fn truncated_or_conflicting_restart_scan_cannot_finalize_cleanup() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        assert_eq!(
            restart_disposition(execution.journal, RestartNameObservation::MissingScanTruncated,),
            RestartDisposition::RetainIndeterminate
        );
        assert_eq!(
            restart_disposition(execution.journal, RestartNameObservation::DifferentIdentity,),
            RestartDisposition::RetainIndeterminate
        );
    }

    #[test]
    fn resolve_publication_cannot_enter_cleanup_prepared_state() {
        let durable = CleanupJournal {
            authority: CleanupAuthority::ResolvePublication,
            ..active_journal()
        };
        assert_eq!(
            plan_cleanup_prepared(durable, artifact()).expect_err("resolve publication"),
            CleanupJournalError::ResolvePublication
        );
    }

    #[test]
    fn artifact_or_generation_change_blocks_cleanup_state_progression() {
        let durable = active_journal();
        let pending = plan_cleanup_prepared(durable, artifact()).expect("plan prepared");
        let advanced = CleanupJournal {
            generation: 1,
            ..durable
        };
        assert_eq!(
            activate_cleanup_prepared(advanced, pending, true).expect_err("stale base"),
            CleanupJournalError::StaleBase
        );

        let mut execution =
            activate_cleanup_prepared(durable, pending, true).expect("activate prepared");
        let foreign_artifact = ArtifactBinding {
            identity: [0x62; 32],
            ..artifact()
        };
        assert_eq!(
            mark_unlink_complete(&mut execution, foreign_artifact).expect_err("artifact mismatch"),
            CleanupJournalError::ArtifactMismatch
        );
        assert!(!execution.unlink_complete);
    }

    #[test]
    fn generation_exhaustion_fails_before_new_cleanup_phase() {
        let durable = CleanupJournal {
            generation: u64::MAX,
            ..active_journal()
        };
        assert_eq!(
            plan_cleanup_prepared(durable, artifact()).expect_err("generation exhausted"),
            CleanupJournalError::GenerationExhausted
        );
    }
}
