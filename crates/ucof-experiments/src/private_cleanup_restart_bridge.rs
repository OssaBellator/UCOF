//! Test-only bridge from bounded restart inventory to cleanup restart authority.
//!
//! This is the integration mapping used after Experiment 0165 filesystem
//! observation. Callers receive the restart disposition rather than interpreting
//! an intermediate inventory observation themselves.

use crate::private_cleanup_restart_inventory::InventoryObservation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreparedCleanupRestartDisposition {
    RetryExactCleanup,
    SyncDirectoryThenFinalize,
    ResolveRenamedPrivate,
    RetainIndeterminate,
}

pub(crate) fn prepared_cleanup_disposition_from_inventory(
    observation: InventoryObservation,
) -> PreparedCleanupRestartDisposition {
    match observation {
        InventoryObservation::ExactIdentity => PreparedCleanupRestartDisposition::RetryExactCleanup,
        InventoryObservation::DifferentIdentity => {
            PreparedCleanupRestartDisposition::RetainIndeterminate
        }
        InventoryObservation::MissingNoMatchingIdentityCompleteScan => {
            PreparedCleanupRestartDisposition::SyncDirectoryThenFinalize
        }
        InventoryObservation::MissingMatchingIdentityElsewhere => {
            PreparedCleanupRestartDisposition::ResolveRenamedPrivate
        }
        InventoryObservation::MissingScanTruncated
        | InventoryObservation::NameMetadataUnreadable => {
            PreparedCleanupRestartDisposition::RetainIndeterminate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_inventory_observation_has_one_fail_closed_restart_disposition() {
        let cases = [
            (
                InventoryObservation::ExactIdentity,
                PreparedCleanupRestartDisposition::RetryExactCleanup,
            ),
            (
                InventoryObservation::DifferentIdentity,
                PreparedCleanupRestartDisposition::RetainIndeterminate,
            ),
            (
                InventoryObservation::MissingNoMatchingIdentityCompleteScan,
                PreparedCleanupRestartDisposition::SyncDirectoryThenFinalize,
            ),
            (
                InventoryObservation::MissingMatchingIdentityElsewhere,
                PreparedCleanupRestartDisposition::ResolveRenamedPrivate,
            ),
            (
                InventoryObservation::MissingScanTruncated,
                PreparedCleanupRestartDisposition::RetainIndeterminate,
            ),
            (
                InventoryObservation::NameMetadataUnreadable,
                PreparedCleanupRestartDisposition::RetainIndeterminate,
            ),
        ];
        for (observation, expected) in cases {
            assert_eq!(
                prepared_cleanup_disposition_from_inventory(observation),
                expected
            );
        }
    }
}
