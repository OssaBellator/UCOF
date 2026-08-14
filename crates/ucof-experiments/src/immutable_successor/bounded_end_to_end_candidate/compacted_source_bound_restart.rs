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
    verify_manifest_bound_stage_identity(
        &persisted_stage,
        manifest,
        restart_limits.max_identity_bytes,
    )
    .map_err(|error| error.to_string())?;
    verify_restart_source_set_authority(journal, manifest, source_set_id, object_count)?;

    let compacted = CompactedNonceJournal::new(journal);
    let mut authority = compacted.recover_authority(trusted_floor)?;
    if authority.durable.generation != crashed_generation {
        return Err("compacted restart journal generation advanced".into());
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
