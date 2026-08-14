//! Test-only restart/discard authority contract for private writer state.
//!
//! This model makes rollback and publication-state rules executable without
//! claiming a production journal MAC, encrypted storage, or durable fsync
//! implementation. The test authenticator is deliberately plumbing-only.

use sha2::{Digest, Sha256};

const JOURNAL_MAGIC: &[u8; 8] = b"UCOFJR01";
const JOURNAL_VERSION: u8 = 1;
const JOURNAL_HEADER_LEN: usize = 96;
const ARTIFACT_LEN: usize = 64;
const TEST_TAG_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactKind {
    SortRun = 1,
    Descriptor = 2,
    Locator = 3,
    PageRef = 4,
    Output = 5,
}

impl ArtifactKind {
    fn decode(value: u8) -> Result<Self, JournalError> {
        match value {
            1 => Ok(Self::SortRun),
            2 => Ok(Self::Descriptor),
            3 => Ok(Self::Locator),
            4 => Ok(Self::PageRef),
            5 => Ok(Self::Output),
            _ => Err(JournalError::Invalid("artifact kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JournalState {
    Prepared = 1,
    Constructing = 2,
    PrivateSynced = 3,
    LinkIndeterminate = 4,
    ParentSyncIndeterminate = 5,
    Durable = 6,
    Discarded = 7,
}

impl JournalState {
    fn decode(value: u8) -> Result<Self, JournalError> {
        match value {
            1 => Ok(Self::Prepared),
            2 => Ok(Self::Constructing),
            3 => Ok(Self::PrivateSynced),
            4 => Ok(Self::LinkIndeterminate),
            5 => Ok(Self::ParentSyncIndeterminate),
            6 => Ok(Self::Durable),
            7 => Ok(Self::Discarded),
            _ => Err(JournalError::Invalid("journal state")),
        }
    }

    fn allows(self, next: Self) -> bool {
        if self == next {
            return matches!(
                self,
                Self::Prepared | Self::Constructing | Self::PrivateSynced
            );
        }
        matches!(
            (self, next),
            (Self::Prepared, Self::Constructing | Self::Discarded)
                | (Self::Constructing, Self::PrivateSynced | Self::Discarded)
                | (
                    Self::PrivateSynced,
                    Self::LinkIndeterminate
                        | Self::ParentSyncIndeterminate
                        | Self::Durable
                        | Self::Discarded
                )
                | (Self::LinkIndeterminate, Self::Durable)
                | (Self::ParentSyncIndeterminate, Self::Durable)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoveryAuthority {
    ResumeOrDiscardPrivate,
    ResolvePublication,
    CleanupDurablePrivate,
    TerminalDiscarded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum JournalError {
    Invalid(&'static str),
    AuthenticationFailed,
    ForeignOperation,
    ForeignKey,
    GenerationRollback,
    NonceRollback,
    InvalidTransition,
    ArtifactLimit,
    DuplicateArtifact,
    GenerationExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JournalArtifact {
    kind: ArtifactKind,
    segment_id: u64,
    bytes: u64,
    sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RestartJournal {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    generation: u64,
    next_nonce: Option<u64>,
    state: JournalState,
    artifacts: Vec<JournalArtifact>,
}

impl RestartJournal {
    fn new(operation_id: [u8; 16], key_id: [u8; 16]) -> Result<Self, JournalError> {
        if operation_id == [0; 16] {
            return Err(JournalError::Invalid("operation id"));
        }
        if key_id == [0; 16] {
            return Err(JournalError::Invalid("key id"));
        }
        Ok(Self {
            operation_id,
            key_id,
            generation: 0,
            next_nonce: Some(0),
            state: JournalState::Prepared,
            artifacts: Vec::new(),
        })
    }

    fn authority(&self) -> RecoveryAuthority {
        match self.state {
            JournalState::Prepared
            | JournalState::Constructing
            | JournalState::PrivateSynced => RecoveryAuthority::ResumeOrDiscardPrivate,
            JournalState::LinkIndeterminate | JournalState::ParentSyncIndeterminate => {
                RecoveryAuthority::ResolvePublication
            }
            JournalState::Durable => RecoveryAuthority::CleanupDurablePrivate,
            JournalState::Discarded => RecoveryAuthority::TerminalDiscarded,
        }
    }

    fn checkpoint(
        &self,
        state: JournalState,
        next_nonce: Option<u64>,
        artifacts: Vec<JournalArtifact>,
        max_artifacts: usize,
    ) -> Result<Self, JournalError> {
        if !self.state.allows(state) {
            return Err(JournalError::InvalidTransition);
        }
        if !nonce_at_least(self.next_nonce, next_nonce) {
            return Err(JournalError::NonceRollback);
        }
        validate_artifacts(&artifacts, max_artifacts)?;
        Ok(Self {
            operation_id: self.operation_id,
            key_id: self.key_id,
            generation: self
                .generation
                .checked_add(1)
                .ok_or(JournalError::GenerationExhausted)?,
            next_nonce,
            state,
            artifacts,
        })
    }
}

fn nonce_at_least(previous: Option<u64>, next: Option<u64>) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) => next >= previous,
        (Some(_), None) | (None, None) => true,
        (None, Some(_)) => false,
    }
}

fn validate_artifacts(artifacts: &[JournalArtifact], max_artifacts: usize) -> Result<(), JournalError> {
    if artifacts.len() > max_artifacts {
        return Err(JournalError::ArtifactLimit);
    }
    for artifact in artifacts {
        if artifact.bytes == 0 {
            return Err(JournalError::Invalid("artifact length"));
        }
    }
    for (index, artifact) in artifacts.iter().enumerate() {
        if artifacts[..index]
            .iter()
            .any(|existing| existing.kind == artifact.kind && existing.segment_id == artifact.segment_id)
        {
            return Err(JournalError::DuplicateArtifact);
        }
    }
    Ok(())
}

trait JournalAuthenticator {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, JournalError>;
    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, JournalError>;
}

fn encode_journal(journal: &RestartJournal) -> Result<Vec<u8>, JournalError> {
    validate_artifacts(&journal.artifacts, u32::MAX as usize)?;
    let artifact_count =
        u32::try_from(journal.artifacts.len()).map_err(|_| JournalError::ArtifactLimit)?;
    let body_len = journal
        .artifacts
        .len()
        .checked_mul(ARTIFACT_LEN)
        .and_then(|bytes| JOURNAL_HEADER_LEN.checked_add(bytes))
        .ok_or(JournalError::Invalid("journal length"))?;
    let mut bytes = vec![0u8; body_len];
    bytes[..8].copy_from_slice(JOURNAL_MAGIC);
    bytes[8] = JOURNAL_VERSION;
    bytes[9] = journal.state as u8;
    bytes[10] = u8::from(journal.next_nonce.is_some());
    bytes[16..32].copy_from_slice(&journal.operation_id);
    bytes[32..48].copy_from_slice(&journal.key_id);
    bytes[48..56].copy_from_slice(&journal.generation.to_le_bytes());
    if let Some(next_nonce) = journal.next_nonce {
        bytes[56..64].copy_from_slice(&next_nonce.to_le_bytes());
    }
    bytes[64..68].copy_from_slice(&artifact_count.to_le_bytes());

    for (index, artifact) in journal.artifacts.iter().enumerate() {
        let offset = JOURNAL_HEADER_LEN + index * ARTIFACT_LEN;
        bytes[offset] = artifact.kind as u8;
        bytes[offset + 8..offset + 16].copy_from_slice(&artifact.segment_id.to_le_bytes());
        bytes[offset + 16..offset + 24].copy_from_slice(&artifact.bytes.to_le_bytes());
        bytes[offset + 24..offset + 56].copy_from_slice(&artifact.sha256);
    }
    Ok(bytes)
}

fn array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], JournalError> {
    bytes
        .get(offset..offset + N)
        .ok_or(JournalError::Invalid("journal field"))?
        .try_into()
        .map_err(|_| JournalError::Invalid("journal field"))
}

fn u32_field(bytes: &[u8], offset: usize) -> Result<u32, JournalError> {
    Ok(u32::from_le_bytes(array(bytes, offset)?))
}

fn u64_field(bytes: &[u8], offset: usize) -> Result<u64, JournalError> {
    Ok(u64::from_le_bytes(array(bytes, offset)?))
}

fn decode_journal(bytes: &[u8], max_artifacts: usize) -> Result<RestartJournal, JournalError> {
    let header = bytes
        .get(..JOURNAL_HEADER_LEN)
        .ok_or(JournalError::Invalid("journal header"))?;
    if &header[..8] != JOURNAL_MAGIC {
        return Err(JournalError::Invalid("journal magic"));
    }
    if header[8] != JOURNAL_VERSION {
        return Err(JournalError::Invalid("journal version"));
    }
    if header[10] > 1
        || header[11..16].iter().any(|byte| *byte != 0)
        || header[68..96].iter().any(|byte| *byte != 0)
    {
        return Err(JournalError::Invalid("journal reserved bytes"));
    }
    let operation_id = array(header, 16)?;
    let key_id = array(header, 32)?;
    if operation_id == [0; 16] || key_id == [0; 16] {
        return Err(JournalError::Invalid("journal identity"));
    }
    let generation = u64_field(header, 48)?;
    let next_nonce = if header[10] == 1 {
        Some(u64_field(header, 56)?)
    } else {
        if u64_field(header, 56)? != 0 {
            return Err(JournalError::Invalid("exhausted nonce encoding"));
        }
        None
    };
    let artifact_count =
        usize::try_from(u32_field(header, 64)?).map_err(|_| JournalError::ArtifactLimit)?;
    if artifact_count > max_artifacts {
        return Err(JournalError::ArtifactLimit);
    }
    let expected_len = artifact_count
        .checked_mul(ARTIFACT_LEN)
        .and_then(|value| JOURNAL_HEADER_LEN.checked_add(value))
        .ok_or(JournalError::Invalid("journal length"))?;
    if bytes.len() != expected_len {
        return Err(JournalError::Invalid("journal exact end"));
    }

    let mut artifacts = Vec::with_capacity(artifact_count);
    for index in 0..artifact_count {
        let offset = JOURNAL_HEADER_LEN + index * ARTIFACT_LEN;
        let record = &bytes[offset..offset + ARTIFACT_LEN];
        if record[1..8].iter().any(|byte| *byte != 0)
            || record[56..64].iter().any(|byte| *byte != 0)
        {
            return Err(JournalError::Invalid("artifact reserved bytes"));
        }
        artifacts.push(JournalArtifact {
            kind: ArtifactKind::decode(record[0])?,
            segment_id: u64_field(record, 8)?,
            bytes: u64_field(record, 16)?,
            sha256: array(record, 24)?,
        });
    }
    validate_artifacts(&artifacts, max_artifacts)?;

    Ok(RestartJournal {
        operation_id,
        key_id,
        generation,
        next_nonce,
        state: JournalState::decode(header[9])?,
        artifacts,
    })
}

fn seal_journal<A: JournalAuthenticator>(
    journal: &RestartJournal,
    authenticator: &A,
) -> Result<Vec<u8>, JournalError> {
    authenticator.seal(&encode_journal(journal)?)
}

fn open_journal<A: JournalAuthenticator>(
    sealed: &[u8],
    authenticator: &A,
    expected_operation: [u8; 16],
    expected_key: [u8; 16],
    minimum_generation: u64,
    minimum_next_nonce: Option<u64>,
    max_artifacts: usize,
) -> Result<RestartJournal, JournalError> {
    let plaintext = authenticator.open(sealed)?;
    let journal = decode_journal(&plaintext, max_artifacts)?;
    if journal.operation_id != expected_operation {
        return Err(JournalError::ForeignOperation);
    }
    if journal.key_id != expected_key {
        return Err(JournalError::ForeignKey);
    }
    if journal.generation < minimum_generation {
        return Err(JournalError::GenerationRollback);
    }
    if !nonce_at_least(minimum_next_nonce, journal.next_nonce) {
        return Err(JournalError::NonceRollback);
    }
    Ok(journal)
}

/// Plumbing-only journal authenticator. It leaves journal plaintext visible and
/// therefore provides no production authentication or confidentiality claim.
struct TestJournalAuthenticator {
    test_key: [u8; 32],
}

impl TestJournalAuthenticator {
    fn tag(&self, plaintext: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"UCOF-TEST-ONLY-RESTART-JOURNAL\0");
        hasher.update(self.test_key);
        hasher.update(plaintext);
        hasher.finalize().into()
    }
}

impl JournalAuthenticator for TestJournalAuthenticator {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, JournalError> {
        let mut sealed = plaintext.to_vec();
        sealed.extend_from_slice(&self.tag(plaintext));
        Ok(sealed)
    }

    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, JournalError> {
        let plaintext_len = sealed
            .len()
            .checked_sub(TEST_TAG_LEN)
            .ok_or(JournalError::AuthenticationFailed)?;
        let (plaintext, tag) = sealed.split_at(plaintext_len);
        if tag != self.tag(plaintext) {
            return Err(JournalError::AuthenticationFailed);
        }
        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authenticator() -> TestJournalAuthenticator {
        TestJournalAuthenticator {
            test_key: [0x44; 32],
        }
    }

    fn artifact(kind: ArtifactKind, segment_id: u64) -> JournalArtifact {
        JournalArtifact {
            kind,
            segment_id,
            bytes: 4096 + segment_id,
            sha256: [u8::try_from(segment_id % 251).expect("digest byte"); 32],
        }
    }

    #[test]
    fn authenticated_round_trip_preserves_restart_authority_and_inventory() {
        let journal = RestartJournal::new([0x11; 16], [0x22; 16])
            .expect("journal")
            .checkpoint(
                JournalState::Constructing,
                Some(17),
                vec![
                    artifact(ArtifactKind::Descriptor, 1),
                    artifact(ArtifactKind::Output, 2),
                ],
                8,
            )
            .expect("checkpoint");
        let sealed = seal_journal(&journal, &authenticator()).expect("seal");
        let opened = open_journal(
            &sealed,
            &authenticator(),
            [0x11; 16],
            [0x22; 16],
            1,
            Some(17),
            8,
        )
        .expect("open");

        assert_eq!(opened, journal);
        assert_eq!(opened.authority(), RecoveryAuthority::ResumeOrDiscardPrivate);
        assert_eq!(
            &sealed[..JOURNAL_HEADER_LEN],
            &encode_journal(&journal).unwrap()[..JOURNAL_HEADER_LEN],
            "test authenticator must remain visibly non-confidential"
        );
    }

    #[test]
    fn tamper_truncation_foreign_identity_and_rollbacks_fail_closed() {
        let journal = RestartJournal::new([0x11; 16], [0x22; 16])
            .expect("journal")
            .checkpoint(
                JournalState::Constructing,
                Some(20),
                vec![artifact(ArtifactKind::Locator, 3)],
                8,
            )
            .expect("checkpoint");
        let sealed = seal_journal(&journal, &authenticator()).expect("seal");

        let mut corrupt = sealed.clone();
        corrupt[48] ^= 1;
        assert_eq!(
            open_journal(
                &corrupt,
                &authenticator(),
                [0x11; 16],
                [0x22; 16],
                0,
                Some(0),
                8,
            )
            .expect_err("tamper"),
            JournalError::AuthenticationFailed
        );
        assert!(open_journal(
            &sealed[..sealed.len() - 1],
            &authenticator(),
            [0x11; 16],
            [0x22; 16],
            0,
            Some(0),
            8,
        )
        .is_err());
        assert_eq!(
            open_journal(
                &sealed,
                &authenticator(),
                [0x33; 16],
                [0x22; 16],
                0,
                Some(0),
                8,
            )
            .expect_err("foreign operation"),
            JournalError::ForeignOperation
        );
        assert_eq!(
            open_journal(
                &sealed,
                &authenticator(),
                [0x11; 16],
                [0x55; 16],
                0,
                Some(0),
                8,
            )
            .expect_err("foreign key"),
            JournalError::ForeignKey
        );
        assert_eq!(
            open_journal(
                &sealed,
                &authenticator(),
                [0x11; 16],
                [0x22; 16],
                2,
                Some(0),
                8,
            )
            .expect_err("generation rollback"),
            JournalError::GenerationRollback
        );
        assert_eq!(
            open_journal(
                &sealed,
                &authenticator(),
                [0x11; 16],
                [0x22; 16],
                1,
                Some(21),
                8,
            )
            .expect_err("nonce rollback"),
            JournalError::NonceRollback
        );
    }

    #[test]
    fn state_machine_preserves_indeterminate_publication_authority() {
        let prepared = RestartJournal::new([0x11; 16], [0x22; 16]).expect("journal");
        assert_eq!(
            prepared.authority(),
            RecoveryAuthority::ResumeOrDiscardPrivate
        );
        let constructing = prepared
            .checkpoint(
                JournalState::Constructing,
                Some(5),
                vec![artifact(ArtifactKind::Output, 1)],
                4,
            )
            .expect("constructing");
        let constructing_checkpoint = constructing
            .checkpoint(
                JournalState::Constructing,
                Some(9),
                vec![artifact(ArtifactKind::Output, 1)],
                4,
            )
            .expect("same-state checkpoint");
        assert_eq!(constructing_checkpoint.generation, 2);
        let synced = constructing_checkpoint
            .checkpoint(
                JournalState::PrivateSynced,
                Some(9),
                vec![artifact(ArtifactKind::Output, 1)],
                4,
            )
            .expect("synced");
        assert_eq!(
            synced
                .checkpoint(
                    JournalState::Constructing,
                    Some(9),
                    vec![artifact(ArtifactKind::Output, 1)],
                    4,
                )
                .expect_err("state regression"),
            JournalError::InvalidTransition
        );
        let indeterminate = synced
            .checkpoint(
                JournalState::LinkIndeterminate,
                Some(9),
                vec![artifact(ArtifactKind::Output, 1)],
                4,
            )
            .expect("indeterminate");
        assert_eq!(
            indeterminate.authority(),
            RecoveryAuthority::ResolvePublication
        );
        assert_eq!(
            indeterminate
                .checkpoint(JournalState::Discarded, Some(9), Vec::new(), 4)
                .expect_err("indeterminate discard"),
            JournalError::InvalidTransition
        );
        let durable = indeterminate
            .checkpoint(
                JournalState::Durable,
                Some(9),
                vec![artifact(ArtifactKind::Output, 1)],
                4,
            )
            .expect("resolved durable");
        assert_eq!(
            durable.authority(),
            RecoveryAuthority::CleanupDurablePrivate
        );
    }

    #[test]
    fn parent_sync_indeterminate_also_forbids_destructive_discard() {
        let journal = RestartJournal::new([0x11; 16], [0x22; 16])
            .expect("journal")
            .checkpoint(JournalState::Constructing, Some(1), Vec::new(), 4)
            .expect("constructing")
            .checkpoint(JournalState::PrivateSynced, Some(1), Vec::new(), 4)
            .expect("synced")
            .checkpoint(
                JournalState::ParentSyncIndeterminate,
                Some(1),
                vec![artifact(ArtifactKind::Output, 7)],
                4,
            )
            .expect("indeterminate");
        assert_eq!(journal.authority(), RecoveryAuthority::ResolvePublication);
        assert_eq!(
            journal
                .checkpoint(JournalState::Discarded, Some(1), Vec::new(), 4)
                .expect_err("must resolve before discard"),
            JournalError::InvalidTransition
        );
    }

    #[test]
    fn nonce_exhaustion_checkpoint_cannot_resume_numeric_counter() {
        let journal = RestartJournal::new([0x11; 16], [0x22; 16])
            .expect("journal")
            .checkpoint(JournalState::Constructing, None, Vec::new(), 4)
            .expect("exhausted checkpoint");
        assert_eq!(
            journal
                .checkpoint(JournalState::Constructing, Some(0), Vec::new(), 4)
                .expect_err("nonce resurrection"),
            JournalError::NonceRollback
        );
        let sealed = seal_journal(&journal, &authenticator()).expect("seal");
        assert_eq!(
            open_journal(
                &sealed,
                &authenticator(),
                [0x11; 16],
                [0x22; 16],
                journal.generation,
                None,
                4,
            )
            .expect("exhausted load")
            .next_nonce,
            None
        );
    }

    #[test]
    fn artifact_inventory_is_bounded_and_unique() {
        let journal = RestartJournal::new([0x11; 16], [0x22; 16]).expect("journal");
        assert_eq!(
            journal
                .checkpoint(
                    JournalState::Constructing,
                    Some(0),
                    vec![
                        artifact(ArtifactKind::Descriptor, 1),
                        artifact(ArtifactKind::Descriptor, 1),
                    ],
                    4,
                )
                .expect_err("duplicate artifact"),
            JournalError::DuplicateArtifact
        );
        assert_eq!(
            journal
                .checkpoint(
                    JournalState::Constructing,
                    Some(0),
                    vec![
                        artifact(ArtifactKind::SortRun, 1),
                        artifact(ArtifactKind::Descriptor, 2),
                    ],
                    1,
                )
                .expect_err("artifact limit"),
            JournalError::ArtifactLimit
        );
    }

    #[test]
    fn exact_end_and_reserved_bytes_are_canonical() {
        let journal = RestartJournal::new([0x11; 16], [0x22; 16])
            .expect("journal")
            .checkpoint(
                JournalState::Constructing,
                Some(3),
                vec![artifact(ArtifactKind::PageRef, 9)],
                4,
            )
            .expect("checkpoint");
        let encoded = encode_journal(&journal).expect("encode");
        assert_eq!(decode_journal(&encoded, 4).expect("decode"), journal);

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            decode_journal(&trailing, 4).expect_err("trailing bytes"),
            JournalError::Invalid("journal exact end")
        );
        let mut reserved = encoded;
        reserved[80] = 1;
        assert_eq!(
            decode_journal(&reserved, 4).expect_err("reserved bytes"),
            JournalError::Invalid("journal reserved bytes")
        );
    }
}
