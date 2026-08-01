#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillRestartJournalPhase {
    StagedFileSynced,
    DestinationLinkCreated,
    DestinationDirectorySynced,
    PrivateNameRetired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillRestartOwnership {
    Owned,
    Foreign,
    Unverifiable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillRestartValidation {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpillRestartJournalEvidence {
    pub phase: SpillRestartJournalPhase,
    pub authenticated: bool,
    pub ownership_matches: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpillRestartFacts {
    pub staged_name_exists: bool,
    pub destination_exists: bool,
    pub staged_ownership: SpillRestartOwnership,
    pub staged_validation: SpillRestartValidation,
    pub destination_validation: SpillRestartValidation,
    pub journal: Option<SpillRestartJournalEvidence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillRestartDisposition {
    NothingToRecover,
    PreserveForeignState,
    RemoveInvalidOwnedStage,
    RetainOwnedStageForRetry,
    PublicationIndeterminate,
    PublishedAndDurable,
    PublishedAndDurableCleanupStage,
    ManualIntervention(&'static str),
}

fn classify_unpublished_stage(facts: SpillRestartFacts) -> SpillRestartDisposition {
    if facts.destination_exists {
        return match facts.destination_validation {
            SpillRestartValidation::Invalid => {
                SpillRestartDisposition::ManualIntervention("invalid destination")
            }
            SpillRestartValidation::Valid | SpillRestartValidation::Unknown => {
                SpillRestartDisposition::PublicationIndeterminate
            }
        };
    }
    if !facts.staged_name_exists {
        return SpillRestartDisposition::NothingToRecover;
    }
    match facts.staged_ownership {
        SpillRestartOwnership::Foreign => SpillRestartDisposition::PreserveForeignState,
        SpillRestartOwnership::Unverifiable => {
            SpillRestartDisposition::ManualIntervention("unverifiable staged ownership")
        }
        SpillRestartOwnership::Owned => match facts.staged_validation {
            SpillRestartValidation::Valid => SpillRestartDisposition::RetainOwnedStageForRetry,
            SpillRestartValidation::Invalid => SpillRestartDisposition::RemoveInvalidOwnedStage,
            SpillRestartValidation::Unknown => {
                SpillRestartDisposition::ManualIntervention("unvalidated owned stage")
            }
        },
    }
}

/// Classifies one fresh-process view of spill publication state without performing cleanup.
///
/// A destination name alone never proves durable publication after restart. Durable authority
/// requires an authenticated, ownership-bound journal at or beyond destination-directory sync and a
/// valid destination matching the journal's expected bytes. Earlier journal phases remain retryable
/// or indeterminate. Contradictory durable records require manual intervention rather than being
/// downgraded or repaired automatically. Foreign and unverifiable private state is never removed.
#[must_use]
pub fn classify_spill_restart(facts: SpillRestartFacts) -> SpillRestartDisposition {
    let Some(journal) = facts.journal else {
        return classify_unpublished_stage(facts);
    };
    if !journal.authenticated {
        return SpillRestartDisposition::ManualIntervention("unauthenticated restart journal");
    }
    if !journal.ownership_matches {
        return SpillRestartDisposition::ManualIntervention("restart journal ownership");
    }

    match journal.phase {
        SpillRestartJournalPhase::StagedFileSynced => {
            if !facts.staged_name_exists {
                return SpillRestartDisposition::ManualIntervention("missing synced stage");
            }
            classify_unpublished_stage(facts)
        }
        SpillRestartJournalPhase::DestinationLinkCreated => {
            if facts.destination_exists {
                return match facts.destination_validation {
                    SpillRestartValidation::Invalid => {
                        SpillRestartDisposition::ManualIntervention("invalid linked destination")
                    }
                    SpillRestartValidation::Valid | SpillRestartValidation::Unknown => {
                        SpillRestartDisposition::PublicationIndeterminate
                    }
                };
            }
            if !facts.staged_name_exists {
                return SpillRestartDisposition::ManualIntervention(
                    "linked destination and stage missing",
                );
            }
            classify_unpublished_stage(facts)
        }
        SpillRestartJournalPhase::DestinationDirectorySynced => {
            if !facts.destination_exists
                || facts.destination_validation != SpillRestartValidation::Valid
            {
                return SpillRestartDisposition::ManualIntervention(
                    "durable destination contradiction",
                );
            }
            if facts.staged_name_exists {
                return match facts.staged_ownership {
                    SpillRestartOwnership::Owned
                        if facts.staged_validation == SpillRestartValidation::Valid =>
                    {
                        SpillRestartDisposition::PublishedAndDurableCleanupStage
                    }
                    SpillRestartOwnership::Foreign => {
                        SpillRestartDisposition::ManualIntervention("foreign stage after durability")
                    }
                    SpillRestartOwnership::Owned | SpillRestartOwnership::Unverifiable => {
                        SpillRestartDisposition::ManualIntervention(
                            "untrusted stage after durability",
                        )
                    }
                };
            }
            SpillRestartDisposition::PublishedAndDurable
        }
        SpillRestartJournalPhase::PrivateNameRetired => {
            if !facts.destination_exists
                || facts.destination_validation != SpillRestartValidation::Valid
            {
                return SpillRestartDisposition::ManualIntervention(
                    "retired publication contradiction",
                );
            }
            if facts.staged_name_exists {
                return SpillRestartDisposition::ManualIntervention(
                    "private name exists after retirement",
                );
            }
            SpillRestartDisposition::PublishedAndDurable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> SpillRestartFacts {
        SpillRestartFacts {
            staged_name_exists: false,
            destination_exists: false,
            staged_ownership: SpillRestartOwnership::Unverifiable,
            staged_validation: SpillRestartValidation::Unknown,
            destination_validation: SpillRestartValidation::Unknown,
            journal: None,
        }
    }

    fn journal(phase: SpillRestartJournalPhase) -> SpillRestartJournalEvidence {
        SpillRestartJournalEvidence {
            phase,
            authenticated: true,
            ownership_matches: true,
        }
    }

    #[test]
    fn destination_without_durable_journal_is_indeterminate() {
        let mut state = facts();
        state.destination_exists = true;
        state.destination_validation = SpillRestartValidation::Valid;
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::PublicationIndeterminate
        );
        state.journal = Some(journal(SpillRestartJournalPhase::DestinationLinkCreated));
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::PublicationIndeterminate
        );
    }

    #[test]
    fn owned_valid_stage_is_retained_and_invalid_stage_is_removed() {
        let mut state = facts();
        state.staged_name_exists = true;
        state.staged_ownership = SpillRestartOwnership::Owned;
        state.staged_validation = SpillRestartValidation::Valid;
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::RetainOwnedStageForRetry
        );
        state.staged_validation = SpillRestartValidation::Invalid;
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::RemoveInvalidOwnedStage
        );
        state.staged_ownership = SpillRestartOwnership::Foreign;
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::PreserveForeignState
        );
    }

    #[test]
    fn synced_destination_authorizes_durable_cleanup_only_for_matching_state() {
        let mut state = facts();
        state.destination_exists = true;
        state.destination_validation = SpillRestartValidation::Valid;
        state.staged_name_exists = true;
        state.staged_ownership = SpillRestartOwnership::Owned;
        state.staged_validation = SpillRestartValidation::Valid;
        state.journal = Some(journal(
            SpillRestartJournalPhase::DestinationDirectorySynced,
        ));
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::PublishedAndDurableCleanupStage
        );
        state.staged_name_exists = false;
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::PublishedAndDurable
        );
        state.destination_exists = false;
        assert!(matches!(
            classify_spill_restart(state),
            SpillRestartDisposition::ManualIntervention(_)
        ));
    }

    #[test]
    fn retired_journal_rejects_reappearing_private_name() {
        let mut state = facts();
        state.destination_exists = true;
        state.destination_validation = SpillRestartValidation::Valid;
        state.journal = Some(journal(SpillRestartJournalPhase::PrivateNameRetired));
        assert_eq!(
            classify_spill_restart(state),
            SpillRestartDisposition::PublishedAndDurable
        );
        state.staged_name_exists = true;
        state.staged_ownership = SpillRestartOwnership::Owned;
        state.staged_validation = SpillRestartValidation::Valid;
        assert!(matches!(
            classify_spill_restart(state),
            SpillRestartDisposition::ManualIntervention(_)
        ));
    }

    #[test]
    fn unauthenticated_or_wrong_owner_journal_never_authorizes_cleanup() {
        let mut state = facts();
        state.staged_name_exists = true;
        state.staged_ownership = SpillRestartOwnership::Owned;
        state.staged_validation = SpillRestartValidation::Invalid;
        state.journal = Some(SpillRestartJournalEvidence {
            phase: SpillRestartJournalPhase::StagedFileSynced,
            authenticated: false,
            ownership_matches: true,
        });
        assert!(matches!(
            classify_spill_restart(state),
            SpillRestartDisposition::ManualIntervention(_)
        ));
        state.journal = Some(SpillRestartJournalEvidence {
            phase: SpillRestartJournalPhase::StagedFileSynced,
            authenticated: true,
            ownership_matches: false,
        });
        assert!(matches!(
            classify_spill_restart(state),
            SpillRestartDisposition::ManualIntervention(_)
        ));
    }
}
