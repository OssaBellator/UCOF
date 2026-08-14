fn stage_and_publish_compacted_source_bound_encrypted_tree_restart<B, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    backend: &mut B,
    sources: &mut [S],
    source_set_id: [u8; 32],
    settings: EncryptedRestartContinuationSettings,
) -> super::CandidateResult<EncryptedTreeRestartPublicationOutcome>
where
    B: super::PersistentStagingBackend,
    S: super::ImmutableStreamingPayloadSource,
{
    let prepared = prepare_compacted_source_bound_encrypted_tree_restart(
        journal,
        stage_directory_path,
        work_directory,
        sources.len(),
        source_set_id,
        settings,
    )?;
    stage_and_publish_prepared_encrypted_tree_restart(
        backend,
        sources,
        work_directory,
        settings,
        prepared,
    )
}

fn prepare_compacted_encrypted_restart_retirement(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    durable: &DurableEncryptedRestartPublication,
    limits: LinuxEncryptedStageRestartLimits,
) -> super::CandidateResult<EncryptedRestartRetirementRecord> {
    let crashed_generation = durable.continuation.crashed_generation;
    let fresh_generation = durable.continuation.fresh_generation;
    let recovery = CompactedNonceJournal::new(journal).scan(None)?;
    if recovery.durable.generation != fresh_generation {
        return Err("compacted retirement fresh generation".into());
    }
    if fresh_generation <= crashed_generation {
        return Err("compacted retirement generation ordering".into());
    }

    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let manifest = load_encrypted_stage_manifest(journal, crashed_generation, role)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "compacted retirement manifest missing".to_owned())?;
    if manifest.key_id != journal.key_id || manifest.nonce_prefix != journal.nonce_prefix {
        return Err("compacted retirement manifest context".into());
    }
    let crashed_nonce_record = load_nonce_generation_record(journal, crashed_generation)
        .map_err(|error| error.to_string())?;
    if crashed_nonce_record.generation != manifest.generation
        || crashed_nonce_record.key_id != manifest.key_id
        || crashed_nonce_record.nonce_prefix != manifest.nonce_prefix
        || crashed_nonce_record.operation_id != manifest.operation_id
    {
        return Err("compacted retirement manifest/nonce context".into());
    }
    if !linux_nonce_at_least(
        crashed_nonce_record.next_unreserved,
        recovery.durable.next_unreserved,
    ) {
        return Err("compacted retirement global nonce authority below crashed stage".into());
    }

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let stage_name = encrypted_stage_file_name(crashed_generation, role);
    let stage_report = scan_encrypted_stage_inventory(
        &stage_directory,
        &stage_name,
        manifest.identity(),
        limits,
    )
    .map_err(|error| error.to_string())?;
    let stage_actual_name = match stage_report.observation {
        crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity => stage_name,
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingMatchingIdentityElsewhere => {
            stage_report
                .matched_name
                .ok_or_else(|| "compacted retirement matched stage name".to_owned())?
        }
        _ => return Err("compacted retirement stage is not exact".into()),
    };
    let stage_identity = exact_file_identity_in_directory(
        &stage_directory,
        &stage_actual_name,
        limits.max_identity_bytes,
    )?;
    if stage_identity != manifest.identity() {
        return Err("compacted retirement stage identity".into());
    }

    let manifest_name = encrypted_stage_manifest_name(crashed_generation, role);
    let manifest_identity = exact_file_identity_in_directory(
        &journal.directory,
        &manifest_name,
        limits.max_identity_bytes,
    )?;
    let record = EncryptedRestartRetirementRecord {
        state: EncryptedRetirementState::Prepared,
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        crashed_generation,
        fresh_generation,
        stage_identity,
        manifest_identity,
        output_length: durable.output_length,
        output_sha256: durable.output_sha256,
    };
    persist_encrypted_retirement_record(journal, record)?;
    Ok(record)
}
