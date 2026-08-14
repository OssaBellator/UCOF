fn write_prepared_encrypted_spill_with_consolidated_encrypted_tree<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    settings: EncryptedSpillPreparedSettings,
    preflight: EncryptedSpillPreflight,
    descriptor_session: &mut DescriptorEncryptionSession,
    tree_session: &mut DescriptorEncryptionSession,
) -> super::CandidateResult<ConsolidatedEncryptedTreeEmissionEvidence>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let EncryptedSpillPreflight {
        descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
        ..
    } = preflight;
    let encrypted_stage =
        transcode_sorted_encrypted_spill_stage(directory, descriptor_stage, descriptor_session)?;
    if encrypted_stage.records() != object_count {
        return Err("encrypted descriptor object count".into());
    }
    encrypted_stage.verify_all(descriptor_session)?;
    let descriptor_stage_bytes = encrypted_stage.bytes()?;
    let descriptor_ciphertext_sha256 = Some(encrypted_stage.ciphertext_sha256()?);
    let descriptor_reader = encrypted_stage.reader(descriptor_session)?;
    let emission = super::PreparedEmission {
        descriptor_stage_bytes,
        descriptor_ciphertext_sha256,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
    };
    let result = write_prepared_from_descriptor_reader_with_encrypted_tree(
        writer,
        sources,
        directory,
        settings.options,
        settings.limits,
        emission,
        descriptor_reader,
        tree_session,
    );
    drop(encrypted_stage);
    result
}

fn write_genesis_sources_end_to_end_encrypted_tree_on_restart_spine<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
    spill_limits: super::BoundedSpillSortLimits,
    descriptor_session: &mut DescriptorEncryptionSession,
    tree_session: &mut DescriptorEncryptionSession,
) -> super::CandidateResult<ConsolidatedEncryptedTreeEmissionEvidence>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let tree_nonces = consolidated_encrypted_tree_stage_record_count(sources.len())?;
    if tree_session.remaining() < tree_nonces {
        return Err("encrypted tree nonce lease capacity".into());
    }
    let preflight = prepare_encrypted_spill_preflight(
        directory,
        sources,
        options,
        limits,
        spill_limits,
        descriptor_session,
    )?;
    write_prepared_encrypted_spill_with_consolidated_encrypted_tree(
        writer,
        sources,
        directory,
        EncryptedSpillPreparedSettings { options, limits },
        preflight,
        descriptor_session,
        tree_session,
    )
}
