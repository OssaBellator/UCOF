#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{
    classify_spill_restart, SpillRestartDisposition, SpillRestartFacts,
    SpillRestartJournalEvidence, SpillRestartJournalPhase, SpillRestartOwnership,
    SpillRestartValidation,
};

fn validation(value: u8) -> SpillRestartValidation {
    match value % 3 {
        0 => SpillRestartValidation::Valid,
        1 => SpillRestartValidation::Invalid,
        _ => SpillRestartValidation::Unknown,
    }
}

fn ownership(value: u8) -> SpillRestartOwnership {
    match value % 3 {
        0 => SpillRestartOwnership::Owned,
        1 => SpillRestartOwnership::Foreign,
        _ => SpillRestartOwnership::Unverifiable,
    }
}

fn phase(value: u8) -> SpillRestartJournalPhase {
    match value % 4 {
        0 => SpillRestartJournalPhase::StagedFileSynced,
        1 => SpillRestartJournalPhase::DestinationLinkCreated,
        2 => SpillRestartJournalPhase::DestinationDirectorySynced,
        _ => SpillRestartJournalPhase::PrivateNameRetired,
    }
}

fuzz_target!(|data: &[u8]| {
    let bytes = |index: usize| data.get(index).copied().unwrap_or(0);
    let journal = if bytes(0) & 1 == 0 {
        None
    } else {
        Some(SpillRestartJournalEvidence {
            phase: phase(bytes(1)),
            authenticated: bytes(2) & 1 != 0,
            ownership_matches: bytes(3) & 1 != 0,
        })
    };
    let facts = SpillRestartFacts {
        staged_name_exists: bytes(4) & 1 != 0,
        destination_exists: bytes(5) & 1 != 0,
        staged_ownership: ownership(bytes(6)),
        staged_validation: validation(bytes(7)),
        destination_validation: validation(bytes(8)),
        journal,
    };
    let disposition = classify_spill_restart(facts);
    assert_eq!(disposition, classify_spill_restart(facts));

    if facts.staged_ownership != SpillRestartOwnership::Owned {
        assert_ne!(
            disposition,
            SpillRestartDisposition::RemoveInvalidOwnedStage
        );
        assert_ne!(
            disposition,
            SpillRestartDisposition::PublishedAndDurableCleanupStage
        );
    }
    if facts.journal.is_none() && facts.destination_exists {
        assert_ne!(disposition, SpillRestartDisposition::PublishedAndDurable);
        assert_ne!(
            disposition,
            SpillRestartDisposition::PublishedAndDurableCleanupStage
        );
    }
    if matches!(
        disposition,
        SpillRestartDisposition::PublishedAndDurable
            | SpillRestartDisposition::PublishedAndDurableCleanupStage
    ) {
        let journal = facts.journal.expect("durability requires journal");
        assert!(journal.authenticated);
        assert!(journal.ownership_matches);
        assert!(matches!(
            journal.phase,
            SpillRestartJournalPhase::DestinationDirectorySynced
                | SpillRestartJournalPhase::PrivateNameRetired
        ));
        assert!(facts.destination_exists);
        assert_eq!(facts.destination_validation, SpillRestartValidation::Valid);
    }
    if disposition == SpillRestartDisposition::RemoveInvalidOwnedStage {
        assert!(facts.staged_name_exists);
        assert!(!facts.destination_exists);
        assert_eq!(facts.staged_ownership, SpillRestartOwnership::Owned);
        assert_eq!(facts.staged_validation, SpillRestartValidation::Invalid);
    }
});
