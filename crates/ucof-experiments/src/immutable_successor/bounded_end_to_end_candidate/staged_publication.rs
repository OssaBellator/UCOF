#[derive(Debug)]
struct BoundedStagedPublicationEvidence {
    storage: PublishedPrivateStoragePlan,
    bounded: EndToEndEvidence,
    output_length: u64,
    output_sha256: [u8; 32],
    outcome: PersistentStagedPublicationOutcome,
    cleanup_error: Option<&'static str>,
}

fn staged_abort<B: PersistentStagingBackend>(backend: &mut B) -> Option<&'static str> {
    backend.abort_private().err()
}

fn stage_and_publish_bounded_sources_candidate<S, B>(
    sources: &mut [S],
    directory: &Path,
    backend: &mut B,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
) -> CandidateResult<BoundedStagedPublicationEvidence>
where
    S: ImmutableStreamingPayloadSource,
    B: PersistentStagingBackend,
{
    let storage = enforce_published_private_storage_limit(
        sources,
        limits,
        spill_limits,
        max_private_storage_bytes,
    )?;
    let preflight = prepare_bounded_preflight(directory, sources, options, limits, spill_limits)?;
    let expected_length =
        u64::try_from(preflight.expected_bytes).map_err(|_| "prepared output length".to_owned())?;
    if expected_length != storage.output_bytes {
        return Err("published quota output length mismatch".into());
    }

    backend
        .begin_private(expected_length)
        .map_err(|error| format!("private publication begin failed: {error}"))?;

    let (bounded, staged_length, staged_sha256) = {
        let mut digest_writer = PersistentPublicationDigestWriter {
            inner: backend,
            hasher: Sha256::new(),
            bytes_written: 0,
        };
        let bounded = match write_prepared_bounded_candidate(
            &mut digest_writer,
            sources,
            directory,
            options,
            limits,
            preflight,
        ) {
            Ok(evidence) => evidence,
            Err(error) => {
                let cleanup = staged_abort(digest_writer.inner);
                return Err(match cleanup {
                    Some(cleanup) => format!(
                        "private bounded construction failed: {error}; cleanup failed: {cleanup}"
                    ),
                    None => format!("private bounded construction failed: {error}"),
                });
            }
        };
        (
            bounded,
            digest_writer.bytes_written,
            <[u8; 32]>::from(digest_writer.hasher.finalize()),
        )
    };

    if staged_length != expected_length {
        let cleanup = staged_abort(backend);
        return Err(match cleanup {
            Some(cleanup) => format!(
                "private bounded staged length mismatch; cleanup failed: {cleanup}"
            ),
            None => "private bounded staged length mismatch".into(),
        });
    }

    if let Err(error) = backend.validate_private(expected_length, staged_sha256) {
        let cleanup = staged_abort(backend);
        return Err(match cleanup {
            Some(cleanup) => format!(
                "private bounded validation failed: {error}; cleanup failed: {cleanup}"
            ),
            None => format!("private bounded validation failed: {error}"),
        });
    }
    if let Err(error) = backend.sync_private() {
        let cleanup = staged_abort(backend);
        return Err(match cleanup {
            Some(cleanup) => format!(
                "private bounded sync failed: {error}; cleanup failed: {cleanup}"
            ),
            None => format!("private bounded sync failed: {error}"),
        });
    }

    match backend.publish_no_replace() {
        Ok(PersistentPublicationLinkOutcome::DestinationExists) => {
            let cleanup_error = staged_abort(backend);
            Ok(BoundedStagedPublicationEvidence {
                storage,
                bounded,
                output_length: staged_length,
                output_sha256: staged_sha256,
                outcome: PersistentStagedPublicationOutcome::NotPublishedDestinationExists,
                cleanup_error,
            })
        }
        Ok(PersistentPublicationLinkOutcome::Indeterminate) => {
            Ok(BoundedStagedPublicationEvidence {
                storage,
                bounded,
                output_length: staged_length,
                output_sha256: staged_sha256,
                outcome: PersistentStagedPublicationOutcome::PublicationIndeterminate {
                    stage: PersistentPublicationStage::PublishLink,
                },
                cleanup_error: None,
            })
        }
        Err(error) => {
            let cleanup = staged_abort(backend);
            Err(match cleanup {
                Some(cleanup) => format!(
                    "private bounded publish failed: {error}; cleanup failed: {cleanup}"
                ),
                None => format!("private bounded publish failed: {error}"),
            })
        }
        Ok(PersistentPublicationLinkOutcome::Linked) => {
            if backend.sync_parent().is_err() {
                return Ok(BoundedStagedPublicationEvidence {
                    storage,
                    bounded,
                    output_length: staged_length,
                    output_sha256: staged_sha256,
                    outcome: PersistentStagedPublicationOutcome::PublicationIndeterminate {
                        stage: PersistentPublicationStage::SyncParent,
                    },
                    cleanup_error: None,
                });
            }
            let cleanup_error = backend.retire_private().err();
            Ok(BoundedStagedPublicationEvidence {
                storage,
                bounded,
                output_length: staged_length,
                output_sha256: staged_sha256,
                outcome: PersistentStagedPublicationOutcome::PublishedAndDurable {
                    cleanup_pending: cleanup_error.is_some(),
                },
                cleanup_error,
            })
        }
    }
}
