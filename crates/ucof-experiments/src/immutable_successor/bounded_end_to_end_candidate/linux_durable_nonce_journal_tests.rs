fn private_directory(label: &str) -> super::TestDirectory {
    let directory = super::TestDirectory::new(label);
    let mut permissions = std::fs::metadata(&directory.0)
        .expect("private directory metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&directory.0, permissions).expect("private directory permissions");
    directory
}

fn journal(
    directory: &Path,
    aes_key: &[u8; 32],
    prefix: [u8; 4],
) -> LinuxDurableNonceJournal {
    LinuxDurableNonceJournal::open(
        directory,
        aes_key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits::default(),
    )
    .expect("open durable nonce journal")
}

fn directory_entry_count(directory: &Path) -> usize {
    std::fs::read_dir(directory)
        .expect("journal directory listing")
        .count()
}

#[test]
fn durable_journal_authorizes_real_encrypted_spill_writer_and_restart_burns_lease() {
    const OBJECTS: u64 = 17;
    let directory = private_directory("durable-nonce-writer");
    let aes_key = [0xc1; 32];
    let prefix = [0x31; 4];
    let lease_size = OBJECTS.checked_mul(2).expect("nonce uses");
    let journal = journal(&directory.0, &aes_key, prefix);
    let mut authority = journal.recover_authority(None).expect("initial authority");
    let mut session = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x41; 16],
            lease_size,
            JournalCommitCut::Complete,
        )
        .expect("durably committed session");

    let limits = super::ImmutableLimits::default();
    let original: Vec<_> = (1..=OBJECTS)
        .rev()
        .map(super::TinySource::new)
        .collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        limits,
    )
    .expect("baseline writer");
    let mut sources = original.clone();
    let mut output = Vec::new();
    let evidence = write_genesis_sources_end_to_end_encrypted_spill_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        super::options(),
        limits,
        super::spill_limits(5, 2),
        &mut session,
    )
    .expect("journal-authorized encrypted writer");
    assert_eq!(output, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(session.remaining(), 0);
    assert_eq!(authority.next_unreserved(), Some(lease_size));
    assert_eq!(session.journal_generation, 1);

    drop(journal);
    let restarted = journal(&directory.0, &aes_key, prefix);
    let mut restarted_authority = restarted
        .recover_authority(None)
        .expect("restart authority");
    assert_eq!(restarted_authority.durable.generation, 1);
    assert_eq!(restarted_authority.next_unreserved(), Some(lease_size));
    let mut second_session = restarted
        .commit_descriptor_session(
            &mut restarted_authority,
            aes_key,
            [0x42; 16],
            lease_size,
            JournalCommitCut::Complete,
        )
        .expect("second operation lease");
    assert_eq!(second_session.lease.first, lease_size);
    assert_eq!(second_session.journal_generation, 2);

    let mut second_sources = original;
    let mut second_output = Vec::new();
    let second_evidence = write_genesis_sources_end_to_end_encrypted_spill_candidate(
        &mut second_output,
        &mut second_sources,
        &directory.0,
        super::options(),
        limits,
        super::spill_limits(7, 3),
        &mut second_session,
    )
    .expect("second journal-authorized encrypted writer");
    assert_eq!(second_output, baseline);
    assert_eq!(second_evidence.output.output, baseline_report);
    assert_eq!(second_session.remaining(), 0);
    assert_eq!(restarted_authority.next_unreserved(), Some(lease_size * 2));
    let final_scan = restarted.scan(None).expect("final scan");
    assert_eq!(final_scan.generations, 2);
    assert_eq!(final_scan.bytes_read, 2 * 128);
    assert_eq!(directory_entry_count(&directory.0), 2);
}

#[test]
fn pre_directory_sync_cuts_never_return_an_issuable_session() {
    let aes_key = [0xd1; 32];
    let prefix = [0x32; 4];
    for (label, cut) in [
        ("journal-cut-write", JournalCommitCut::AfterWriteBeforeFileSync),
        (
            "journal-cut-file-sync",
            JournalCommitCut::AfterFileSyncBeforeDirectorySync,
        ),
    ] {
        let directory = private_directory(label);
        let journal = journal(&directory.0, &aes_key, prefix);
        let mut authority = journal.recover_authority(None).expect("initial authority");
        let error = match journal.commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x51; 16],
            4,
            cut,
        ) {
            Ok(_) => panic!("cut must not activate session"),
            Err(error) => error,
        };
        assert_eq!(error, LinuxNonceJournalError::InjectedCut(cut));
        assert_eq!(authority.durable, DurableNonceState::initial());

        let visible = journal.scan(None).expect("visible candidate is safe to burn");
        assert_eq!(visible.durable.generation, 1);
        assert_eq!(visible.durable.next_unreserved, Some(4));
        assert_eq!(directory_entry_count(&directory.0), 1);
    }
}

#[test]
fn lost_pre_sync_candidate_can_be_reused_only_because_no_session_was_issued() {
    let directory = private_directory("journal-lost-pre-sync");
    let aes_key = [0xe1; 32];
    let prefix = [0x33; 4];
    let journal = journal(&directory.0, &aes_key, prefix);
    let mut authority = journal.recover_authority(None).expect("initial authority");
    let error = match journal.commit_descriptor_session(
        &mut authority,
        aes_key,
        [0x61; 16],
        4,
        JournalCommitCut::AfterWriteBeforeFileSync,
    ) {
        Ok(_) => panic!("pre-sync cut must not activate session"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        LinuxNonceJournalError::InjectedCut(JournalCommitCut::AfterWriteBeforeFileSync)
    );
    assert_eq!(authority.next_unreserved(), Some(0));

    std::fs::remove_file(directory.0.join(linux_nonce_journal_name(1)))
        .expect("simulate lost uncommitted generation");
    journal.directory.sync_all().expect("sync simulated loss");
    let mut recovered = journal
        .recover_authority(None)
        .expect("recover initial after lost candidate");
    let session = journal
        .commit_descriptor_session(
            &mut recovered,
            aes_key,
            [0x62; 16],
            4,
            JournalCommitCut::Complete,
        )
        .expect("reuse never-issued counters");
    assert_eq!(session.lease.first, 0);
    assert_eq!(recovered.next_unreserved(), Some(4));
    assert_eq!(directory_entry_count(&directory.0), 1);
}

#[test]
fn tamper_and_generation_gap_fail_closed() {
    let directory = private_directory("journal-tamper-gap");
    let aes_key = [0xf1; 32];
    let prefix = [0x34; 4];
    let journal = journal(&directory.0, &aes_key, prefix);
    let mut authority = journal.recover_authority(None).expect("initial authority");
    let _first = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x71; 16],
            4,
            JournalCommitCut::Complete,
        )
        .expect("first lease");
    let _second = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x72; 16],
            4,
            JournalCommitCut::Complete,
        )
        .expect("second lease");

    let first_path = directory.0.join(linux_nonce_journal_name(1));
    let mut first = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&first_path)
        .expect("open first journal");
    std::io::Seek::seek(&mut first, std::io::SeekFrom::Start(64)).expect("seek journal");
    let mut byte = [0u8; 1];
    first.read_exact(&mut byte).expect("read journal byte");
    byte[0] ^= 0x80;
    std::io::Seek::seek(&mut first, std::io::SeekFrom::Start(64)).expect("seek journal");
    first.write_all(&byte).expect("tamper journal");
    first.sync_all().expect("sync tamper");
    assert_eq!(
        journal.scan(None).expect_err("tamper must fail"),
        LinuxNonceJournalError::AuthenticationFailed
    );

    drop(first);
    std::fs::remove_file(&first_path).expect("remove first generation");
    journal.directory.sync_all().expect("sync gap");
    assert_eq!(
        journal.scan(None).expect_err("gap must fail"),
        LinuxNonceJournalError::GenerationGap
    );
    assert_eq!(directory_entry_count(&directory.0), 1);
}

#[test]
fn external_floor_detects_tail_rollback_that_self_journal_cannot_prove() {
    let directory = private_directory("journal-tail-rollback");
    let aes_key = [0xa2; 32];
    let prefix = [0x35; 4];
    let journal = journal(&directory.0, &aes_key, prefix);
    let mut authority = journal.recover_authority(None).expect("initial authority");
    let _first = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x81; 16],
            4,
            JournalCommitCut::Complete,
        )
        .expect("first lease");
    let _second = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x82; 16],
            4,
            JournalCommitCut::Complete,
        )
        .expect("second lease");
    let floor = TrustedNonceFloor::from_authority(&authority);
    assert_eq!(floor.generation, 2);
    assert_eq!(floor.next_unreserved, Some(8));

    std::fs::remove_file(directory.0.join(linux_nonce_journal_name(2)))
        .expect("simulate authenticated tail rollback");
    journal.directory.sync_all().expect("sync rollback");
    let self_only = journal
        .recover_authority(None)
        .expect("self journal cannot know deleted tail existed");
    assert_eq!(self_only.durable.generation, 1);
    assert_eq!(self_only.next_unreserved(), Some(4));
    let error = match journal.recover_authority(Some(floor)) {
        Ok(_) => panic!("trusted floor must detect rollback"),
        Err(error) => error,
    };
    assert_eq!(error, LinuxNonceJournalError::Rollback);
    assert_eq!(directory_entry_count(&directory.0), 1);
}

#[test]
fn key_prefix_and_stale_authority_are_fail_closed() {
    let directory = private_directory("journal-binding-stale");
    let aes_key = [0xb2; 32];
    let prefix = [0x36; 4];
    let journal = journal(&directory.0, &aes_key, prefix);
    let mut first_authority = journal.recover_authority(None).expect("first authority");
    let mut stale_authority = journal.recover_authority(None).expect("stale authority");
    let _session = journal
        .commit_descriptor_session(
            &mut first_authority,
            aes_key,
            [0x91; 16],
            4,
            JournalCommitCut::Complete,
        )
        .expect("winning lease");
    let stale_error = match journal.commit_descriptor_session(
        &mut stale_authority,
        aes_key,
        [0x92; 16],
        4,
        JournalCommitCut::Complete,
    ) {
        Ok(_) => panic!("stale authority must not activate session"),
        Err(error) => error,
    };
    assert_eq!(stale_error, LinuxNonceJournalError::StaleAuthority);
    let key_error = match journal.commit_descriptor_session(
        &mut first_authority,
        [0xc2; 32],
        [0x93; 16],
        4,
        JournalCommitCut::Complete,
    ) {
        Ok(_) => panic!("wrong AES key must not activate session"),
        Err(error) => error,
    };
    assert_eq!(key_error, LinuxNonceJournalError::ForeignKey);

    drop(journal);
    let wrong_prefix = journal(&directory.0, &aes_key, [0x37; 4]);
    let prefix_error = match wrong_prefix.recover_authority(None) {
        Ok(_) => panic!("wrong prefix must not recover authority"),
        Err(error) => error,
    };
    assert_eq!(prefix_error, LinuxNonceJournalError::ForeignNoncePrefix);
    assert_eq!(directory_entry_count(&directory.0), 1);
}
