struct RestartPublicationTestBackend {
    private: Vec<u8>,
    destination: Option<Vec<u8>>,
    begun: bool,
    expected_length: u64,
    link: super::PersistentPublicationLinkOutcome,
    fail_sync_parent: bool,
    fail_retire: bool,
    aborted: bool,
}

impl RestartPublicationTestBackend {
    fn new(link: super::PersistentPublicationLinkOutcome) -> Self {
        Self {
            private: Vec::new(),
            destination: None,
            begun: false,
            expected_length: 0,
            link,
            fail_sync_parent: false,
            fail_retire: false,
            aborted: false,
        }
    }
}

impl Write for RestartPublicationTestBackend {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if !self.begun {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private publication not begun",
            ));
        }
        self.private.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl super::PersistentStagingBackend for RestartPublicationTestBackend {
    fn begin_private(&mut self, expected_length: u64) -> Result<(), &'static str> {
        self.begun = true;
        self.expected_length = expected_length;
        Ok(())
    }

    fn validate_private(
        &mut self,
        expected_length: u64,
        expected_sha256: [u8; 32],
    ) -> Result<(), &'static str> {
        if !self.begun
            || self.expected_length != expected_length
            || u64::try_from(self.private.len()).map_err(|_| "private length")? != expected_length
            || <[u8; 32]>::from(Sha256::digest(&self.private)) != expected_sha256
        {
            return Err("private validation");
        }
        Ok(())
    }

    fn sync_private(&mut self) -> Result<(), &'static str> {
        if !self.begun {
            return Err("private not begun");
        }
        Ok(())
    }

    fn publish_no_replace(
        &mut self,
    ) -> Result<super::PersistentPublicationLinkOutcome, &'static str> {
        match self.link {
            super::PersistentPublicationLinkOutcome::Linked => {
                self.destination = Some(self.private.clone());
                Ok(super::PersistentPublicationLinkOutcome::Linked)
            }
            super::PersistentPublicationLinkOutcome::DestinationExists => {
                Ok(super::PersistentPublicationLinkOutcome::DestinationExists)
            }
            super::PersistentPublicationLinkOutcome::Indeterminate => {
                Ok(super::PersistentPublicationLinkOutcome::Indeterminate)
            }
        }
    }

    fn sync_parent(&mut self) -> Result<(), &'static str> {
        if self.fail_sync_parent {
            Err("parent sync")
        } else {
            Ok(())
        }
    }

    fn retire_private(&mut self) -> Result<(), &'static str> {
        if self.fail_retire {
            return Err("retire private");
        }
        self.private.clear();
        self.begun = false;
        Ok(())
    }

    fn abort_private(&mut self) -> Result<(), &'static str> {
        self.private.clear();
        self.begun = false;
        self.aborted = true;
        Ok(())
    }
}

fn restart_publication_fixture(
    label: &str,
    object_count: u64,
) -> (
    super::TestDirectory,
    super::TestDirectory,
    super::TestDirectory,
    [u8; 32],
    [u8; 4],
    LinuxEncryptedStageRestartLimits,
    Vec<super::TinySource>,
    Vec<u8>,
    super::ImmutableSourceStreamingWriteReport,
) {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            label,
            object_count,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist restart publication stage");
    let work_directory = super::TestDirectory::new(&format!("{label}-work"));
    let sources: Vec<_> = (1..=object_count)
        .rev()
        .map(super::TinySource::new)
        .collect();
    let mut baseline_sources = sources.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("restart publication baseline");
    (
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        sources,
        baseline,
        baseline_report,
    )
}

#[test]
fn only_parent_synced_restart_publication_returns_durable_cleanup_capability() {
    const OBJECTS: u64 = 11;
    let (
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        baseline,
        baseline_report,
    ) = restart_publication_fixture("restart-publication-durable", OBJECTS);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut backend = RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let outcome = stage_and_publish_verified_encrypted_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut backend,
        &mut sources,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x83; 16],
        },
    )
    .expect("durable restart publication");

    let EncryptedRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("only parent-synced publication may be durable");
    };
    assert_eq!(backend.destination.as_deref(), Some(baseline.as_slice()));
    assert_eq!(durable.continuation.output.output, baseline_report);
    assert_eq!(durable.output_length, u64::try_from(baseline.len()).unwrap());
    assert_eq!(durable.output_sha256, <[u8; 32]>::from(Sha256::digest(&baseline)));
    assert!(!durable.cleanup_pending);
    assert!(backend.private.is_empty());
    let authority = journal.recover_authority(None).expect("durable publication authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(authority.next_unreserved(), Some(OBJECTS * 3));
    work_directory.assert_empty();
}

#[test]
fn destination_exists_never_returns_durable_restart_publication() {
    const OBJECTS: u64 = 7;
    let (
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        _,
        _,
    ) = restart_publication_fixture("restart-publication-exists", OBJECTS);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut backend = RestartPublicationTestBackend::new(
        super::PersistentPublicationLinkOutcome::DestinationExists,
    );
    backend.destination = Some(b"existing destination".to_vec());
    let outcome = stage_and_publish_verified_encrypted_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut backend,
        &mut sources,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x84; 16],
        },
    )
    .expect("destination-exists restart publication");
    assert!(matches!(
        outcome,
        EncryptedRestartPublicationOutcome::NotPublishedDestinationExists
    ));
    assert_eq!(backend.destination.as_deref(), Some(b"existing destination".as_slice()));
    assert!(backend.private.is_empty());
    assert!(backend.aborted);
    let authority = journal.recover_authority(None).expect("destination-exists authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(authority.next_unreserved(), Some(OBJECTS * 3));
    work_directory.assert_empty();
}

#[test]
fn failed_parent_sync_is_indeterminate_and_cannot_be_cleanup_authority() {
    const OBJECTS: u64 = 5;
    let (
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        baseline,
        _,
    ) = restart_publication_fixture("restart-publication-parent-sync", OBJECTS);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut backend = RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    backend.fail_sync_parent = true;
    let outcome = stage_and_publish_verified_encrypted_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut backend,
        &mut sources,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x85; 16],
        },
    )
    .expect("parent-sync indeterminate restart publication");
    assert!(matches!(
        outcome,
        EncryptedRestartPublicationOutcome::PublicationIndeterminate {
            stage: super::PersistentPublicationStage::SyncParent
        }
    ));
    assert_eq!(backend.destination.as_deref(), Some(baseline.as_slice()));
    assert_eq!(backend.private.as_slice(), baseline.as_slice());
    assert!(!backend.aborted);
    let authority = journal.recover_authority(None).expect("parent-sync authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(authority.next_unreserved(), Some(OBJECTS * 3));
    work_directory.assert_empty();
}
