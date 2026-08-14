fn classify_encrypted_spill_restart_compacted(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    aes_key: &[u8; 32],
    generation: u64,
    trusted_floor: Option<TrustedNonceFloor>,
    limits: LinuxEncryptedStageRestartLimits,
) -> super::CandidateResult<(
    EncryptedStageRestartDisposition,
    EncryptedStageRestartScanStats,
)> {
    limits.validate().map_err(|error| error.to_string())?;
    linux_nonce_validate_component(stage_directory_path).map_err(|error| error.to_string())?;
    if journal.key_id != linux_nonce_key_id(aes_key) {
        return Err("compacted restart foreign AES key".into());
    }

    let compacted = CompactedNonceJournal::new(journal);
    let recovery = compacted.scan(trusted_floor)?;
    let Some(manifest) = load_encrypted_stage_manifest(
        journal,
        generation,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )?
    else {
        return Ok((
            EncryptedStageRestartDisposition::NoDurableManifestRestartWork,
            EncryptedStageRestartScanStats {
                scanned_entries: 0,
                identity_bytes: 0,
            },
        ));
    };
    if manifest.generation != generation
        || manifest.key_id != journal.key_id
        || manifest.nonce_prefix != journal.nonce_prefix
    {
        return Ok((
            EncryptedStageRestartDisposition::RetainIndeterminate,
            EncryptedStageRestartScanStats {
                scanned_entries: 0,
                identity_bytes: 0,
            },
        ));
    }
    if recovery.durable.generation != generation {
        return Ok((
            EncryptedStageRestartDisposition::RetainIndeterminate,
            EncryptedStageRestartScanStats {
                scanned_entries: 0,
                identity_bytes: 0,
            },
        ));
    }
    let nonce_record = load_nonce_generation_record(journal, generation)
        .map_err(|error| error.to_string())?;
    if nonce_record.operation_id != manifest.operation_id
        || nonce_record.lease_first != manifest.lease_first
        || nonce_record.lease_last != manifest.lease_last
        || nonce_record.next_unreserved != manifest.next_unreserved
    {
        return Ok((
            EncryptedStageRestartDisposition::RetainIndeterminate,
            EncryptedStageRestartScanStats {
                scanned_entries: 0,
                identity_bytes: 0,
            },
        ));
    }
    let expected_object_count = usize::try_from(manifest.object_count)
        .map_err(|_| "compacted restart object count".to_owned())?;
    let expected_stage_bytes = u64::try_from(expected_object_count)
        .map_err(|_| "compacted restart object count".to_owned())?
        .checked_mul(
            u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_RECORD_BYTES)
                .expect("encrypted spill record width fits u64"),
        )
        .ok_or_else(|| "compacted restart stage bytes".to_owned())?;
    if manifest.stage_bytes != expected_stage_bytes || manifest.stage_bytes > limits.max_identity_bytes {
        return Ok((
            EncryptedStageRestartDisposition::RetainIndeterminate,
            EncryptedStageRestartScanStats {
                scanned_entries: 0,
                identity_bytes: 0,
            },
        ));
    }

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let (entries, mut stats) = bounded_restart_scan_entries(
        &stage_directory,
        limits.max_directory_entries,
        limits.max_name_bytes,
    )?;
    let expected_stage_name = encrypted_restart_stage_name(generation);
    let mut exact_match = None;
    let mut renamed_match = None;
    for name in entries {
        let Some(file) = linux_nonce_open_relative_readonly(&stage_directory, &name)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let identity = encrypted_stage_strong_file_identity(
            file,
            limits.max_identity_bytes,
        )?;
        stats.identity_bytes = stats
            .identity_bytes
            .checked_add(identity.bytes)
            .ok_or_else(|| "compacted restart identity bytes".to_owned())?;
        if stats.identity_bytes > limits.max_total_identity_bytes {
            return Err("compacted restart identity byte limit".into());
        }
        let matches = identity.bytes == manifest.stage_bytes && identity.sha256 == manifest.stage_sha256;
        if name == expected_stage_name {
            if !matches {
                return Ok((EncryptedStageRestartDisposition::RetainIndeterminate, stats));
            }
            exact_match = Some(name);
        } else if matches {
            if renamed_match.is_some() {
                return Ok((EncryptedStageRestartDisposition::RetainIndeterminate, stats));
            }
            renamed_match = Some(name);
        }
    }

    if exact_match.is_some() && renamed_match.is_some() {
        return Ok((EncryptedStageRestartDisposition::RetainIndeterminate, stats));
    }
    if exact_match.is_some() {
        return Ok((
            EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease {
                object_count: expected_object_count,
            },
            stats,
        ));
    }
    if let Some(actual_name) = renamed_match {
        return Ok((
            EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease {
                object_count: expected_object_count,
                actual_name,
            },
            stats,
        ));
    }
    Ok((EncryptedStageRestartDisposition::StageAbsentRestartWork, stats))
}
