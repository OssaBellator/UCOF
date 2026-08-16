fn ensure_compacted_restart_publication_directory_headroom(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<()> {
    require_linux_nonce_journal_metadata_slots(journal, 2, "compacted publication")
        .map_err(|_| "compacted publication retirement directory headroom".to_owned())
}

fn ensure_compacted_restart_prepared_directory_headroom(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<()> {
    require_linux_nonce_journal_metadata_slots(journal, 1, "compacted retirement")
        .map_err(|_| "compacted retirement Prepared directory headroom".to_owned())
}

fn prepare_compacted_source_bound_encrypted_tree_restart(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    source_count: usize,
    source_set_id: [u8; 32],
    settings: EncryptedRestartContinuationSettings,
) -> super::CandidateResult<PreparedEncryptedTreeRestartContinuation> {
    if source_set_id == [0; 32] {
        return Err("compacted restart source-set identity".into());
    }
    let EncryptedRestartContinuationSettings {
        aes_key,
        crashed_generation,
        trusted_floor,
        restart_limits,
        options,
        limits,
        fresh_operation_id,
    } = settings;
    let (disposition, _) = classify_encrypted_spill_restart_compacted(
        journal,
        stage_directory_path,
        &aes_key,
        crashed_generation,
        trusted_floor,
        restart_limits,
    )?;
    let object_count = match &disposition {
        EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease { object_count }
        | EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease {
            object_count, ..
        } => *object_count,
        EncryptedStageRestartDisposition::NoDurableManifestRestartWork => {
            return Err("compacted restart stage has no durable manifest".into())
        }
        EncryptedStageRestartDisposition::StageAbsentRestartWork => {
            return Err("compacted restart stage is absent".into())
        }
        EncryptedStageRestartDisposition::RetainIndeterminate => {
            return Err("compacted restart stage is indeterminate".into())
        }
    };
    if source_count != object_count {
        return Err("compacted restart source count".into());
    }

    let tree_nonce_count = consolidated_encrypted_tree_stage_record_count(object_count)?;
    let object_nonce_count =
        u64::try_from(object_count).map_err(|_| "compacted restart object nonce count")?;
    let fresh_lease_size = object_nonce_count
        .checked_add(tree_nonce_count)
        .ok_or_else(|| "compacted restart lease size".to_owned())?;

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let (manifest, crashed_nonce_record, persisted_stage) = open_verified_restart_stage(
        journal,
        &stage_directory,
        crashed_generation,
        &disposition,
    )?;
    if crashed_nonce_record.generation != manifest.generation
        || crashed_nonce_record.key_id != manifest.key_id
        || crashed_nonce_record.nonce_prefix != manifest.nonce_prefix
        || crashed_nonce_record.operation_id != manifest.operation_id
    {
        return Err("compacted restart manifest/nonce context".into());
    }
    verify_manifest_bound_stage_identity(
        &persisted_stage,
        manifest,
        restart_limits.max_identity_bytes,
    )
    .map_err(|error| error.to_string())?;
    verify_restart_source_set_authority(journal, manifest, source_set_id, object_count)?;

    let compacted = CompactedNonceJournal::new(journal);
    let mut authority = compacted.recover_authority(trusted_floor)?;
    if authority.durable.generation < crashed_generation
        || !linux_nonce_at_least(
            crashed_nonce_record.next_unreserved,
            authority.durable.next_unreserved,
        )
    {
        return Err("compacted restart journal authority below crashed generation".into());
    }
    let mut fresh_session = compacted.commit_descriptor_session(
        &mut authority,
        aes_key,
        fresh_operation_id,
        fresh_lease_size,
        JournalCommitCut::Complete,
    )?;
    let fresh_lease_first = fresh_session.lease.first;
    let fresh_lease_last = fresh_session.lease.last;

    let (retained_stage, recovered) = transcode_restart_spill_with_fresh_session(
        work_directory,
        &persisted_stage,
        manifest,
        crashed_nonce_record,
        &aes_key,
        RestartTranscodeSettings { options, limits },
        &mut fresh_session,
    )?;
    if fresh_session.remaining() != tree_nonce_count {
        return Err("compacted restart tree lease remainder".into());
    }
    if retained_stage.records() != object_count {
        return Err("compacted restart retained descriptor count".into());
    }
    retained_stage.verify_all(&fresh_session)?;

    Ok(PreparedEncryptedTreeRestartContinuation {
        retained_stage,
        fresh_session,
        recovered,
        crashed_generation,
        crashed_lease_first: crashed_nonce_record.lease_first,
        crashed_lease_last: crashed_nonce_record.lease_last,
        fresh_lease_first,
        fresh_lease_last,
        persisted_spill_sha256: manifest.stage_sha256,
        tree_nonce_count,
    })
}

fn continue_compacted_source_bound_encrypted_tree_restart<W, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    writer: &mut W,
    sources: &mut [S],
    source_set_id: [u8; 32],
    settings: EncryptedRestartContinuationSettings,
) -> super::CandidateResult<EncryptedTreeRestartContinuationEvidence>
where
    W: Write,
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
    emit_prepared_encrypted_tree_restart_continuation(
        writer,
        sources,
        work_directory,
        settings.options,
        settings.limits,
        prepared,
    )
}

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
    ensure_compacted_restart_publication_directory_headroom(journal)?;
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
    let mutation = acquire_restart_metadata_mutation_lock(journal)
        .map_err(|error| error.to_string())?;
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
    ensure_compacted_restart_prepared_directory_headroom(journal)?;
    persist_encrypted_retirement_record(journal, &mutation, record)?;
    Ok(record)
}
