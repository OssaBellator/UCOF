#[derive(Debug)]
struct DurableEncryptedTreeRestartPublication {
    durable: DurableEncryptedRestartPublication,
    tree_stage_ciphertext_sha256: [u8; 32],
}

#[derive(Debug)]
enum EncryptedTreeRestartPublicationOutcome {
    NotPublishedDestinationExists,
    PublishedAndDurable(Box<DurableEncryptedTreeRestartPublication>),
    PublicationIndeterminate {
        stage: super::PersistentPublicationStage,
    },
}

fn stage_and_publish_prepared_encrypted_tree_restart<B, S>(
    backend: &mut B,
    sources: &mut [S],
    work_directory: &Path,
    settings: EncryptedRestartContinuationSettings,
    prepared: PreparedEncryptedTreeRestartContinuation,
) -> super::CandidateResult<EncryptedTreeRestartPublicationOutcome>
where
    B: super::PersistentStagingBackend,
    S: super::ImmutableStreamingPayloadSource,
{
    let expected_length = u64::try_from(prepared.recovered.expected_bytes)
        .map_err(|_| "restart publication length".to_owned())?;
    backend
        .begin_private(expected_length)
        .map_err(|error| format!("restart publication begin private: {error}"))?;

    let (continuation, staged_length, staged_sha256) = {
        let mut digest_writer = super::PersistentPublicationDigestWriter {
            inner: backend,
            hasher: Sha256::new(),
            bytes_written: 0,
        };
        let continuation = match emit_prepared_encrypted_tree_restart_continuation(
            &mut digest_writer,
            sources,
            work_directory,
            settings.options,
            settings.limits,
            prepared,
        ) {
            Ok(evidence) => evidence,
            Err(error) => {
                let cleanup_error = publication_abort_cleanup(digest_writer.inner);
                return Err(match cleanup_error {
                    Some(cleanup) => format!(
                        "encrypted-tree restart publication copy failed: {error}; cleanup failed: {cleanup}"
                    ),
                    None => format!("encrypted-tree restart publication copy failed: {error}"),
                });
            }
        };
        (
            continuation,
            digest_writer.bytes_written,
            <[u8; 32]>::from(digest_writer.hasher.finalize()),
        )
    };

    if staged_length != expected_length {
        let _ = backend.abort_private();
        return Err("encrypted-tree restart publication staged length".into());
    }
    backend
        .validate_private(expected_length, staged_sha256)
        .map_err(|error| {
            let cleanup_error = publication_abort_cleanup(backend);
            match cleanup_error {
                Some(cleanup) => format!(
                    "encrypted-tree restart publication validate private: {error}; cleanup failed: {cleanup}"
                ),
                None => format!("encrypted-tree restart publication validate private: {error}"),
            }
        })?;
    backend.sync_private().map_err(|error| {
        let cleanup_error = publication_abort_cleanup(backend);
        match cleanup_error {
            Some(cleanup) => format!(
                "encrypted-tree restart publication sync private: {error}; cleanup failed: {cleanup}"
            ),
            None => format!("encrypted-tree restart publication sync private: {error}"),
        }
    })?;

    match backend.publish_no_replace() {
        Ok(super::PersistentPublicationLinkOutcome::DestinationExists) => {
            let _ = publication_abort_cleanup(backend);
            Ok(EncryptedTreeRestartPublicationOutcome::NotPublishedDestinationExists)
        }
        Ok(super::PersistentPublicationLinkOutcome::Indeterminate) => {
            Ok(EncryptedTreeRestartPublicationOutcome::PublicationIndeterminate {
                stage: super::PersistentPublicationStage::PublishLink,
            })
        }
        Err(error) => {
            let cleanup_error = publication_abort_cleanup(backend);
            Err(match cleanup_error {
                Some(cleanup) => format!(
                    "encrypted-tree restart publication publish link: {error}; cleanup failed: {cleanup}"
                ),
                None => format!("encrypted-tree restart publication publish link: {error}"),
            })
        }
        Ok(super::PersistentPublicationLinkOutcome::Linked) => {
            if backend.sync_parent().is_err() {
                return Ok(EncryptedTreeRestartPublicationOutcome::PublicationIndeterminate {
                    stage: super::PersistentPublicationStage::SyncParent,
                });
            }
            let cleanup_pending = backend.retire_private().is_err();
            let EncryptedTreeRestartContinuationEvidence {
                output,
                tree_stage_ciphertext_sha256,
                crashed_generation,
                fresh_generation,
                crashed_lease_first,
                crashed_lease_last,
                fresh_lease_first,
                fresh_lease_last,
                persisted_spill_sha256,
            } = continuation;
            let legacy_authority = EncryptedRestartContinuationEvidence {
                output,
                crashed_generation,
                fresh_generation,
                crashed_lease_first,
                crashed_lease_last,
                fresh_lease_first,
                fresh_lease_last,
                persisted_spill_sha256,
            };
            Ok(EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(
                Box::new(DurableEncryptedTreeRestartPublication {
                    durable: DurableEncryptedRestartPublication {
                        continuation: legacy_authority,
                        output_length: expected_length,
                        output_sha256: staged_sha256,
                        cleanup_pending,
                    },
                    tree_stage_ciphertext_sha256,
                }),
            ))
        }
    }
}

fn stage_and_publish_verified_encrypted_tree_restart<B, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    backend: &mut B,
    sources: &mut [S],
    settings: EncryptedRestartContinuationSettings,
) -> super::CandidateResult<EncryptedTreeRestartPublicationOutcome>
where
    B: super::PersistentStagingBackend,
    S: super::ImmutableStreamingPayloadSource,
{
    let prepared = prepare_verified_encrypted_spill_with_fresh_tree_lease(
        journal,
        stage_directory_path,
        work_directory,
        sources.len(),
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
