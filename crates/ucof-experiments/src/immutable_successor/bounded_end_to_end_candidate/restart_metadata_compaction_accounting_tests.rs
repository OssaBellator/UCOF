#[test]
fn compacted_scan_reports_nonce_and_checkpoint_authenticated_bytes() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-byte-accounting", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    let compacted = CompactedNonceJournal::new(&journal);

    let before = compacted.scan(None).expect("scan nonce bytes before compaction");
    assert_eq!(before.journal_records, 2);
    assert_eq!(
        before.bytes_read,
        2 * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width")
    );

    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact byte-accounting fixture");
    let after = compacted.scan(None).expect("scan checkpoint bytes after compaction");
    assert_eq!(after.journal_records, 0);
    assert_eq!(after.checkpoint_generation, Some(2));
    assert_eq!(
        after.bytes_read,
        u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width")
    );
}

#[test]
fn checkpoint_overhead_does_not_consume_legacy_journal_byte_capacity_on_retry() {
    let directory = private_directory("nonce-compaction-journal-cap-retry");
    let key = [0xb3; 32];
    let prefix = [0x49; 4];
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 16,
            max_generations: 2,
            max_journal_bytes: 2
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: 64,
        },
    )
    .expect("open exact-cap journal");
    let mut authority = journal
        .recover_authority(None)
        .expect("initial exact-cap authority");
    journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x21; 16],
            5,
            JournalCommitCut::Complete,
        )
        .expect("first exact-cap generation");
    journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x22; 16],
            7,
            JournalCommitCut::Complete,
        )
        .expect("second exact-cap generation");

    let report = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointDirectorySyncBeforePrune,
    )
    .expect("checkpoint exact-cap journal before prune");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 0);

    let coexistence = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("checkpoint plus full legacy journal must remain recoverable");
    assert_eq!(coexistence.durable.generation, 2);
    assert_eq!(coexistence.durable.next_unreserved, Some(12));
    assert_eq!(coexistence.journal_records, 0);
    assert_eq!(
        coexistence.bytes_read,
        2 * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width")
            + u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width")
    );

    let retried = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry exact-cap compaction");
    assert_eq!(retried.pruned_nonce_records, 2);
    let final_recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("recover exact-cap journal after prune");
    assert_eq!(final_recovery.durable.generation, 2);
    assert_eq!(final_recovery.durable.next_unreserved, Some(12));
    assert_eq!(final_recovery.journal_records, 0);
}

#[test]
fn authenticated_checkpoint_gets_exactly_one_transient_directory_entry_at_ceiling() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-directory-ceiling", &[5, 7]);
    assert_eq!(directory_entry_count(&directory.0), 2);
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 2,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open exact-directory-ceiling journal");

    let error = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync,
    )
    .expect_err("stop after checkpoint creates the one transient directory entry");
    assert!(error.contains("after checkpoint file sync"));
    assert_eq!(directory_entry_count(&directory.0), 3);
    assert!(directory.0.join(nonce_compaction_name(2)).exists());

    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("authenticated checkpoint permits one transient entry over configured cap");
    assert_eq!(recovery.durable.generation, 2);
    assert_eq!(recovery.checkpoint_generation, Some(2));
    let inventory = scan_compacted_persistent_inventory(&journal)
        .expect("quota inventory permits same authenticated checkpoint headroom");
    assert_eq!(inventory.nonce_records, 2);
    assert_eq!(inventory.checkpoint_records, 1);

    let retry = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry exact-directory-ceiling checkpoint through prune");
    assert_eq!(retry.checkpoint_generation, 2);
    assert_eq!(retry.pruned_nonce_records, 2);
    assert_eq!(directory_entry_count(&directory.0), 1);
    assert_eq!(
        CompactedNonceJournal::new(&journal)
            .scan(None)
            .expect("scan after directory-ceiling retry")
            .durable
            .generation,
        2
    );
}

#[test]
fn unrelated_extra_directory_entry_does_not_receive_checkpoint_headroom() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-unrelated-directory-extra", &[5, 7]);
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 2,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open unrelated-extra constrained journal");
    std::fs::write(directory.0.join("unrelated-extra"), b"x")
        .expect("create unrelated directory entry");
    assert_eq!(directory_entry_count(&directory.0), 3);

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("unrelated extra entry must not receive checkpoint headroom");
    assert!(error.contains("compacted nonce directory entry limit"));
    let inventory_error = scan_compacted_persistent_inventory(&journal)
        .expect_err("quota inventory must reject unrelated extra entry too");
    assert!(
        inventory_error.contains("compacted inventory directory entry limit")
            || inventory_error.contains("compacted inventory unrecognized entry")
    );
    assert!(!directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn compacted_nonce_commit_reserves_one_directory_slot_for_next_checkpoint() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-commit-headroom", &[5, 7]);
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 2,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open compacted commit-headroom journal");
    let compacted = CompactedNonceJournal::new(&journal);
    let mut authority = compacted
        .recover_authority(None)
        .expect("recover full-directory authority");
    let error = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x31; 16],
            3,
            JournalCommitCut::Complete,
        )
        .expect_err("full directory must reserve checkpoint headroom");
    assert!(error.contains("compacted nonce checkpoint directory headroom"));
    assert_eq!(authority.durable.generation, 2);
    assert!(!directory.0.join(linux_nonce_journal_name(3)).exists());

    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact full directory to one checkpoint");
    assert_eq!(directory_entry_count(&directory.0), 1);
    let mut authority = compacted
        .recover_authority(None)
        .expect("recover post-compaction authority");
    let generation_three = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x32; 16],
            3,
            JournalCommitCut::Complete,
        )
        .expect("one ordinary generation may consume the reserved slot");
    assert_eq!(generation_three.journal_generation, 3);
    drop(generation_three);
    assert_eq!(directory_entry_count(&directory.0), 2);

    let error = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x33; 16],
            3,
            JournalCommitCut::Complete,
        )
        .expect_err("second ordinary generation must wait for compaction");
    assert!(error.contains("compacted nonce checkpoint directory headroom"));
    assert_eq!(authority.durable.generation, 3);
    assert!(!directory.0.join(linux_nonce_journal_name(4)).exists());

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("new checkpoint may use the reserved transient entry");
    assert_eq!(report.checkpoint_generation, 3);
    assert_eq!(report.pruned_nonce_records, 1);
    assert_eq!(report.pruned_old_checkpoints, 1);
    assert_eq!(directory_entry_count(&directory.0), 1);
    let generation_four = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x34; 16],
            3,
            JournalCommitCut::Complete,
        )
        .expect("allocation resumes after compaction restores headroom");
    assert_eq!(generation_four.journal_generation, 4);
}

#[test]
fn checkpoint_does_not_lend_transient_headroom_to_unknown_entry() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-checkpoint-plus-unknown", &[5, 7]);
    let initial = open_journal(&directory.0, &key, prefix);
    compact_restart_metadata(&initial, None, RestartMetadataCompactionCut::Complete)
        .expect("reduce fixture to one authenticated checkpoint");
    assert_eq!(directory_entry_count(&directory.0), 1);
    assert!(directory.0.join(nonce_compaction_name(2)).exists());

    let constrained = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 1,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open one-entry checkpoint directory");
    std::fs::write(directory.0.join("unrelated-extra"), b"x")
        .expect("create unknown entry beside checkpoint");
    assert_eq!(directory_entry_count(&directory.0), 2);

    let error = CompactedNonceJournal::new(&constrained)
        .scan(None)
        .expect_err("checkpoint must not lend its transient slot to unknown content");
    assert!(error.contains("compacted nonce directory entry limit"));
    let inventory_error = scan_compacted_persistent_inventory(&constrained)
        .expect_err("quota inventory must reject unknown content beside checkpoint too");
    assert!(
        inventory_error.contains("compacted inventory directory entry limit")
            || inventory_error.contains("compacted inventory unrecognized entry")
    );
    assert!(directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn compacted_inventory_rejects_authenticated_foreign_stage_manifest_context() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xb5; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compacted-inventory-foreign-manifest",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let original = load_encrypted_stage_manifest(&journal, 1, role)
        .expect("load original compacted inventory manifest")
        .expect("original compacted inventory manifest");
    let name = encrypted_stage_manifest_name(1, role);
    std::fs::remove_file(journal_directory.0.join(&name))
        .expect("remove original compacted inventory manifest");
    let forged = LinuxEncryptedStageManifest {
        key_id: [0xf5; 16],
        ..original
    };
    let sealed = seal_encrypted_stage_manifest(&journal, forged)
        .expect("seal foreign-context manifest with local authentication key");
    let path = linux_nonce_procfd_child(&journal.directory, &name)
        .expect("foreign compacted inventory manifest path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .expect("create foreign compacted inventory manifest");
    file.write_all(&sealed)
        .expect("write foreign compacted inventory manifest");
    file.flush()
        .expect("flush foreign compacted inventory manifest");
    file.sync_all()
        .expect("sync foreign compacted inventory manifest");
    journal
        .directory
        .sync_all()
        .expect("sync foreign compacted inventory manifest directory");

    let error = scan_compacted_persistent_inventory(&journal)
        .expect_err("authenticated foreign stage manifest must fail quota inventory");
    assert!(error.contains("compacted inventory stage manifest context"));
    assert!(!journal_directory.0.join(nonce_compaction_name(1)).exists());
}

#[test]
fn compacted_inventory_rejects_unrecognized_entry_even_below_directory_limit() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("compacted-inventory-unrecognized-below-cap", &[5]);
    let journal = open_journal(&directory.0, &key, prefix);
    std::fs::write(directory.0.join("unrecognized-private-metadata"), vec![0x5a; 4096])
        .expect("create unrecognized private metadata file");
    assert!(directory_entry_count(&directory.0) < journal.limits.max_directory_entries);

    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("lightweight recovery remains tolerant of bounded unrelated entry");
    assert_eq!(recovery.durable.generation, 1);

    let error = scan_compacted_persistent_inventory(&journal)
        .expect_err("quota inventory must not ignore unrecognized bytes below entry cap");
    assert!(error.contains("compacted inventory unrecognized entry"));
    let plan_error = compaction_storage_plan(&journal)
        .expect_err("standalone compaction quota plan must reject unaccounted bytes too");
    assert!(plan_error.contains("compacted inventory unrecognized entry"));
    assert!(!directory.0.join(nonce_compaction_name(1)).exists());
}
