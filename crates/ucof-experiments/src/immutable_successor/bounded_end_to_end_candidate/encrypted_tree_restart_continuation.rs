#[derive(Debug)]
struct EncryptedTreeRestartContinuationEvidence {
    output: super::EndToEndEvidence,
    tree_stage_ciphertext_sha256: [u8; 32],
    crashed_generation: u64,
    fresh_generation: u64,
    crashed_lease_first: u64,
    crashed_lease_last: u64,
    fresh_lease_first: u64,
    fresh_lease_last: u64,
    persisted_spill_sha256: [u8; 32],
}

struct PreparedEncryptedTreeRestartContinuation {
    retained_stage: EncryptedDescriptorStage,
    fresh_session: DescriptorEncryptionSession,
    recovered: RestartRecoveredPreflight,
    crashed_generation: u64,
    crashed_lease_first: u64,
    crashed_lease_last: u64,
    fresh_lease_first: u64,
    fresh_lease_last: u64,
    persisted_spill_sha256: [u8; 32],
    tree_nonce_count: u64,
}

fn prepare_verified_encrypted_spill_with_fresh_tree_lease(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    source_count: usize,
    settings: EncryptedRestartContinuationSettings,
) -> super::CandidateResult<PreparedEncryptedTreeRestartContinuation> {
    let EncryptedRestartContinuationSettings {
        aes_key,
        crashed_generation,
        trusted_floor,
        restart_limits,
        options,
        limits,
        fresh_operation_id,
    } = settings;
    let (disposition, _) = classify_encrypted_spill_restart(
        journal,
        stage_directory_path,
        &aes_key,
        crashed_generation,
        trusted_floor,
        restart_limits,
    )
    .map_err(|error| error.to_string())?;
    let object_count = match &disposition {
        EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease { object_count }
        | EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease {
            object_count, ..
        } => *object_count,
        EncryptedStageRestartDisposition::NoDurableManifestRestartWork => {
            return Err("restart stage has no durable manifest".into())
        }
        EncryptedStageRestartDisposition::StageAbsentRestartWork => {
            return Err("restart stage is absent".into())
        }
        EncryptedStageRestartDisposition::RetainIndeterminate => {
            return Err("restart stage is indeterminate".into())
        }
    };
    if source_count != object_count {
        return Err("restart source count".into());
    }

    let tree_nonce_count = consolidated_encrypted_tree_stage_record_count(object_count)?;
    let object_nonce_count =
        u64::try_from(object_count).map_err(|_| "fresh restart object nonce count".to_owned())?;
    let fresh_lease_size = object_nonce_count
        .checked_add(tree_nonce_count)
        .ok_or_else(|| "fresh encrypted-tree restart lease size".to_owned())?;

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let (manifest, crashed_nonce_record, persisted_stage) = open_verified_restart_stage(
        journal,
        &stage_directory,
        crashed_generation,
        &disposition,
    )?;
    verify_manifest_bound_stage_identity(
        &persisted_stage,
        manifest,
        restart_limits.max_identity_bytes,
    )
    .map_err(|error| error.to_string())?;

    let mut authority = journal
        .recover_authority(trusted_floor)
        .map_err(|error| error.to_string())?;
    if authority.durable.generation != crashed_generation {
        return Err("restart journal generation advanced".into());
    }
    let mut fresh_session = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            fresh_operation_id,
            fresh_lease_size,
            JournalCommitCut::Complete,
        )
        .map_err(|error| error.to_string())?;
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
        return Err("fresh encrypted-tree restart lease remainder".into());
    }
    if retained_stage.records() != object_count {
        return Err("restart retained descriptor count".into());
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

fn emit_prepared_encrypted_tree_restart_continuation<W, S>(
    writer: &mut W,
    sources: &mut [S],
    work_directory: &Path,
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
    prepared: PreparedEncryptedTreeRestartContinuation,
) -> super::CandidateResult<EncryptedTreeRestartContinuationEvidence>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let PreparedEncryptedTreeRestartContinuation {
        retained_stage,
        mut fresh_session,
        recovered,
        crashed_generation,
        crashed_lease_first,
        crashed_lease_last,
        fresh_lease_first,
        fresh_lease_last,
        persisted_spill_sha256,
        tree_nonce_count,
    } = prepared;
    if sources.len() != recovered.object_count {
        return Err("restart source count".into());
    }
    if fresh_session.remaining() != tree_nonce_count {
        return Err("fresh encrypted-tree restart lease before emission".into());
    }

    let descriptor_stage_bytes = retained_stage.bytes()?;
    let descriptor_ciphertext_sha256 = Some(retained_stage.ciphertext_sha256()?);
    let descriptor_reader = retained_stage.reader(&fresh_session)?;
    let descriptor_spill = reconstructed_restart_spill_report(
        recovered.object_count,
        u64::try_from(recovered.object_count)
            .map_err(|_| "restart spill record count".to_owned())?
            .checked_mul(
                u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES)
                    .expect("encrypted spill width fits u64"),
            )
            .ok_or_else(|| "restart spill stage length".to_owned())?,
        persisted_spill_sha256,
    )?;
    let emission = super::PreparedEmission {
        descriptor_stage_bytes,
        descriptor_ciphertext_sha256,
        descriptor_spill,
        expected_bytes: recovered.expected_bytes,
        expected_pages: recovered.expected_pages,
        expected_root_level: recovered.expected_root_level,
        largest_source_buffer: recovered.largest_source_buffer,
        version_checks: recovered.version_checks,
        object_count: recovered.object_count,
    };
    let encrypted_tree = write_prepared_from_descriptor_reader_with_encrypted_tree(
        writer,
        sources,
        work_directory,
        options,
        limits,
        emission,
        descriptor_reader,
        &mut fresh_session,
    )?;
    if fresh_session.remaining() != 0 {
        return Err("fresh encrypted-tree restart lease not exhausted".into());
    }
    drop(retained_stage);

    Ok(EncryptedTreeRestartContinuationEvidence {
        output: encrypted_tree.base,
        tree_stage_ciphertext_sha256: encrypted_tree.tree_stage_ciphertext_sha256,
        crashed_generation,
        fresh_generation: fresh_session.journal_generation,
        crashed_lease_first,
        crashed_lease_last,
        fresh_lease_first,
        fresh_lease_last,
        persisted_spill_sha256,
    })
}

fn continue_verified_encrypted_spill_with_fresh_tree_lease<W, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    writer: &mut W,
    sources: &mut [S],
    settings: EncryptedRestartContinuationSettings,
) -> super::CandidateResult<EncryptedTreeRestartContinuationEvidence>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let prepared = prepare_verified_encrypted_spill_with_fresh_tree_lease(
        journal,
        stage_directory_path,
        work_directory,
        sources.len(),
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
