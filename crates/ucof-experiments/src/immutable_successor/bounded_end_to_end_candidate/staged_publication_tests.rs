struct MemoryPublicationBackend {
    private: Vec<u8>,
    destination: Option<Vec<u8>>,
    expected_length: u64,
    begun: bool,
    link: PersistentPublicationLinkOutcome,
    fail_parent_sync: bool,
    aborts: usize,
}

impl MemoryPublicationBackend {
    fn new(link: PersistentPublicationLinkOutcome) -> Self {
        Self {
            private: Vec::new(),
            destination: None,
            expected_length: 0,
            begun: false,
            link,
            fail_parent_sync: false,
            aborts: 0,
        }
    }
}

impl Write for MemoryPublicationBackend {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if !self.begun {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private output not begun",
            ));
        }
        self.private.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl PersistentStagingBackend for MemoryPublicationBackend {
    fn begin_private(&mut self, expected_length: u64) -> Result<(), &'static str> {
        self.private.clear();
        self.expected_length = expected_length;
        self.begun = true;
        Ok(())
    }

    fn validate_private(
        &mut self,
        expected_length: u64,
        expected_sha256: [u8; 32],
    ) -> Result<(), &'static str> {
        if !self.begun
            || expected_length != self.expected_length
            || u64::try_from(self.private.len()).map_err(|_| "private length")? != expected_length
            || <[u8; 32]>::from(Sha256::digest(&self.private)) != expected_sha256
        {
            return Err("private validation");
        }
        Ok(())
    }

    fn sync_private(&mut self) -> Result<(), &'static str> {
        Ok(())
    }

    fn publish_no_replace(&mut self) -> Result<PersistentPublicationLinkOutcome, &'static str> {
        match self.link {
            PersistentPublicationLinkOutcome::Linked => {
                if self.destination.is_some() {
                    return Ok(PersistentPublicationLinkOutcome::DestinationExists);
                }
                self.destination = Some(self.private.clone());
            }
            PersistentPublicationLinkOutcome::DestinationExists => {}
            PersistentPublicationLinkOutcome::Indeterminate => {}
        }
        Ok(self.link)
    }

    fn sync_parent(&mut self) -> Result<(), &'static str> {
        if self.fail_parent_sync {
            Err("parent sync")
        } else {
            Ok(())
        }
    }

    fn retire_private(&mut self) -> Result<(), &'static str> {
        self.private.clear();
        self.begun = false;
        Ok(())
    }

    fn abort_private(&mut self) -> Result<(), &'static str> {
        self.private.clear();
        self.begun = false;
        self.aborts += 1;
        Ok(())
    }
}

#[test]
fn bounded_genesis_publishes_exact_canonical_artifact_through_private_backend() {
    const OBJECTS: u64 = 401;
    let limits = ImmutableLimits::default();
    let spill = spill_limits(17, 3);
    let original: Vec<_> = (1..=OBJECTS).rev().map(TinySource::new).collect();

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        options(),
        limits,
    )
    .expect("baseline writer");

    let plan = published_private_storage_plan(&original, limits, spill).expect("published quota");
    let directory = TestDirectory::new("published-success");
    let mut sources = original;
    let mut backend = MemoryPublicationBackend::new(PersistentPublicationLinkOutcome::Linked);
    let evidence = stage_and_publish_bounded_sources_candidate(
        &mut sources,
        &directory.0,
        &mut backend,
        options(),
        limits,
        spill,
        plan.required_bytes,
    )
    .expect("bounded staged publication");

    assert_eq!(evidence.storage, plan);
    assert_eq!(evidence.bounded.output, baseline_report);
    assert_eq!(evidence.output_length, u64::try_from(baseline.len()).unwrap());
    assert_eq!(evidence.output_sha256, <[u8; 32]>::from(Sha256::digest(&baseline)));
    assert_eq!(
        evidence.outcome,
        PersistentStagedPublicationOutcome::PublishedAndDurable {
            cleanup_pending: false
        }
    );
    assert_eq!(evidence.cleanup_error, None);
    assert_eq!(backend.destination.as_deref(), Some(baseline.as_slice()));
    assert!(backend.private.is_empty());
    directory.assert_empty();
}

#[test]
fn post_payload_freshness_failure_aborts_private_output_and_keeps_destination_absent() {
    let limits = ImmutableLimits::default();
    let spill = spill_limits(1, 2);
    let mut sources = [ChangingVersionSource::new()];
    let plan = published_private_storage_plan(&sources, limits, spill).expect("published quota");
    let directory = TestDirectory::new("published-version-failure");
    let mut backend = MemoryPublicationBackend::new(PersistentPublicationLinkOutcome::Linked);

    let error = stage_and_publish_bounded_sources_candidate(
        &mut sources,
        &directory.0,
        &mut backend,
        options(),
        limits,
        spill,
        plan.required_bytes,
    )
    .expect_err("post-payload version failure must abort private output");

    assert!(error.contains("version"));
    assert_eq!(sources[0].version_calls, 3);
    assert_eq!(backend.aborts, 1);
    assert!(backend.private.is_empty());
    assert!(backend.destination.is_none());
    directory.assert_empty();
}

#[test]
fn published_quota_one_byte_short_rejects_before_working_or_output_staging() {
    let limits = ImmutableLimits::default();
    let spill = spill_limits(17, 3);
    let mut sources: Vec<_> = (1..=401).rev().map(TinySource::new).collect();
    let plan = published_private_storage_plan(&sources, limits, spill).expect("published quota");
    let directory = TestDirectory::new("published-quota-short");
    let mut backend = MemoryPublicationBackend::new(PersistentPublicationLinkOutcome::Linked);

    let error = stage_and_publish_bounded_sources_candidate(
        &mut sources,
        &directory.0,
        &mut backend,
        options(),
        limits,
        spill,
        plan.required_bytes - 1,
    )
    .expect_err("short published quota must fail");

    assert!(error.contains("published private storage limit"));
    assert!(!backend.begun);
    assert!(backend.private.is_empty());
    assert!(backend.destination.is_none());
    directory.assert_empty();
}

#[test]
fn destination_exists_preserves_existing_bytes_and_aborts_private_artifact() {
    let limits = ImmutableLimits::default();
    let spill = spill_limits(17, 3);
    let original: Vec<_> = (1..=401).rev().map(TinySource::new).collect();
    let plan = published_private_storage_plan(&original, limits, spill).expect("published quota");
    let directory = TestDirectory::new("published-exists");
    let mut sources = original;
    let mut backend = MemoryPublicationBackend::new(PersistentPublicationLinkOutcome::DestinationExists);
    backend.destination = Some(b"existing".to_vec());

    let evidence = stage_and_publish_bounded_sources_candidate(
        &mut sources,
        &directory.0,
        &mut backend,
        options(),
        limits,
        spill,
        plan.required_bytes,
    )
    .expect("destination exists outcome");

    assert_eq!(
        evidence.outcome,
        PersistentStagedPublicationOutcome::NotPublishedDestinationExists
    );
    assert_eq!(backend.destination.as_deref(), Some(b"existing".as_slice()));
    assert_eq!(backend.aborts, 1);
    assert!(backend.private.is_empty());
    directory.assert_empty();
}

#[test]
fn failed_parent_sync_is_indeterminate_and_retains_private_state() {
    let limits = ImmutableLimits::default();
    let spill = spill_limits(17, 3);
    let original: Vec<_> = (1..=401).rev().map(TinySource::new).collect();
    let plan = published_private_storage_plan(&original, limits, spill).expect("published quota");
    let directory = TestDirectory::new("published-parent-sync");
    let mut sources = original;
    let mut backend = MemoryPublicationBackend::new(PersistentPublicationLinkOutcome::Linked);
    backend.fail_parent_sync = true;

    let evidence = stage_and_publish_bounded_sources_candidate(
        &mut sources,
        &directory.0,
        &mut backend,
        options(),
        limits,
        spill,
        plan.required_bytes,
    )
    .expect("parent sync indeterminate outcome");

    assert_eq!(
        evidence.outcome,
        PersistentStagedPublicationOutcome::PublicationIndeterminate {
            stage: PersistentPublicationStage::SyncParent
        }
    );
    assert!(backend.destination.is_some());
    assert!(!backend.private.is_empty());
    assert!(backend.begun);
    directory.assert_empty();
}
