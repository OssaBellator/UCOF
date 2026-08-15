#[test]
fn journal_metadata_capacity_guard_counts_existing_entries_before_creation() {
    let directory = private_directory("journal-metadata-capacity-guard");
    let aes_key = [0xd1; 32];
    let prefix = [0x71; 4];
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &aes_key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 1,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open one-entry metadata-capacity journal");

    require_linux_nonce_journal_metadata_slots(&journal, 1, "metadata test")
        .expect("one free journal entry is available");
    std::fs::write(directory.0.join("occupied"), b"x")
        .expect("occupy configured journal entry");
    let error = require_linux_nonce_journal_metadata_slots(&journal, 1, "metadata test")
        .expect_err("no additional metadata slot remains");
    assert!(error.contains("metadata test directory headroom"));
}
