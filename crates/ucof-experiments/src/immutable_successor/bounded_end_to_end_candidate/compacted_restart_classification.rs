fn classify_encrypted_spill_restart_compacted(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    aes_key: &[u8; 32],
    generation: u64,
    trusted_floor: Option<TrustedNonceFloor>,
    limits: LinuxEncryptedStageRestartLimits,
) -> super::CandidateResult<(
    EncryptedStageRestartDisposition,
    Option<EncryptedStageInventoryReport>,
)> {
    let recovery = CompactedNonceJournal::new(journal).scan(trusted_floor)?;
    if recovery.durable.generation < generation {
        return Ok((
            EncryptedStageRestartDisposition::NoDurableManifestRestartWork,
            None,
        ));
    }

    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let Some(manifest) = load_encrypted_stage_manifest(journal, generation, role)
        .map_err(|error| error.to_string())?
    else {
        return Ok((
            EncryptedStageRestartDisposition::NoDurableManifestRestartWork,
            None,
        ));
    };
    if manifest.key_id != linux_nonce_key_id(aes_key) {
        return Err("compacted restart foreign AES key".into());
    }
    if manifest.nonce_prefix != journal.nonce_prefix {
        return Err("compacted restart foreign nonce prefix".into());
    }

    let nonce_record = load_nonce_generation_record(journal, generation)
        .map_err(|error| error.to_string())?;
    if nonce_record.operation_id != manifest.operation_id {
        return Err("compacted restart foreign operation".into());
    }
    if nonce_record.generation != manifest.generation
        || nonce_record.key_id != manifest.key_id
        || nonce_record.nonce_prefix != manifest.nonce_prefix
    {
        return Err("compacted restart nonce/manifest context mismatch".into());
    }
    if !linux_nonce_at_least(nonce_record.next_unreserved, recovery.durable.next_unreserved) {
        return Err("compacted restart global nonce authority below crashed generation".into());
    }

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let expected_name = encrypted_stage_file_name(generation, role);
    let report = scan_encrypted_stage_inventory(
        &stage_directory,
        &expected_name,
        manifest.identity(),
        limits,
    )
    .map_err(|error| error.to_string())?;

    let disposition = match report.observation {
        crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity
        | crate::private_cleanup_restart_inventory::InventoryObservation::MissingMatchingIdentityElsewhere => {
            let actual_name = report
                .matched_name
                .clone()
                .ok_or_else(|| "compacted restart matched stage name".to_owned())?;
            let file = linux_nonce_open_relative_readonly(&stage_directory, &actual_name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compacted restart matched stage disappeared".to_owned())?;
            verify_manifest_bound_stage_identity(&file, manifest, limits.max_identity_bytes)
                .map_err(|error| error.to_string())?;
            let object_count = verify_persisted_sorted_encrypted_spill(
                &file,
                manifest,
                nonce_record,
                aes_key,
                limits,
            )
            .map_err(|error| error.to_string())?;
            if report.observation
                == crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity
            {
                EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease { object_count }
            } else {
                EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease {
                    object_count,
                    actual_name,
                }
            }
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingNoMatchingIdentityCompleteScan => {
            EncryptedStageRestartDisposition::StageAbsentRestartWork
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::DifferentIdentity
        | crate::private_cleanup_restart_inventory::InventoryObservation::MissingScanTruncated
        | crate::private_cleanup_restart_inventory::InventoryObservation::NameMetadataUnreadable => {
            EncryptedStageRestartDisposition::RetainIndeterminate
        }
    };
    Ok((disposition, Some(report)))
}
