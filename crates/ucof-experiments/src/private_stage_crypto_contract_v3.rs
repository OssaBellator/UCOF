//! Test-only contract model for authenticated private-stage records.
//!
//! This module deliberately does not provide production encryption. The
//! `PrivateStageAead` trait is the boundary for a vetted AEAD implementation.
//! Tests use an authenticated passthrough that leaves plaintext visible so the
//! research evidence cannot be mistaken for a confidentiality claim.

use sha2::{Digest, Sha256};

const RECORD_MAGIC: &[u8; 8] = b"UCOFSTG1";
const RECORD_VERSION: u8 = 1;
const AAD_LEN: usize = 72;
const HEADER_LEN: usize = 96;
const TEST_TAG_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StageKind {
    SortRun = 1,
    Descriptor = 2,
    Locator = 3,
    PageRef = 4,
}

impl StageKind {
    fn decode(value: u8) -> Result<Self, CryptoContractError> {
        match value {
            1 => Ok(Self::SortRun),
            2 => Ok(Self::Descriptor),
            3 => Ok(Self::Locator),
            4 => Ok(Self::PageRef),
            _ => Err(CryptoContractError::Invalid("stage kind")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CryptoContractError {
    Invalid(&'static str),
    NonceExhausted,
    UnauthenticatedCheckpoint,
    AuthenticationFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NonceCheckpoint {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    prefix: [u8; 4],
    next_counter: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OperationCryptoContext {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    prefix: [u8; 4],
    next_counter: Option<u64>,
}

impl OperationCryptoContext {
    fn new(
        operation_id: [u8; 16],
        key_id: [u8; 16],
        prefix: [u8; 4],
    ) -> Result<Self, CryptoContractError> {
        if operation_id == [0; 16] {
            return Err(CryptoContractError::Invalid("operation id"));
        }
        if key_id == [0; 16] {
            return Err(CryptoContractError::Invalid("key id"));
        }
        Ok(Self {
            operation_id,
            key_id,
            prefix,
            next_counter: Some(0),
        })
    }

    fn from_counter_for_test(
        operation_id: [u8; 16],
        key_id: [u8; 16],
        prefix: [u8; 4],
        next_counter: u64,
    ) -> Result<Self, CryptoContractError> {
        let mut context = Self::new(operation_id, key_id, prefix)?;
        context.next_counter = Some(next_counter);
        Ok(context)
    }

    fn checkpoint(&self) -> NonceCheckpoint {
        NonceCheckpoint {
            operation_id: self.operation_id,
            key_id: self.key_id,
            prefix: self.prefix,
            next_counter: self.next_counter,
        }
    }

    fn resume(
        checkpoint: NonceCheckpoint,
        authenticated: bool,
    ) -> Result<Self, CryptoContractError> {
        if !authenticated {
            return Err(CryptoContractError::UnauthenticatedCheckpoint);
        }
        if checkpoint.operation_id == [0; 16] || checkpoint.key_id == [0; 16] {
            return Err(CryptoContractError::Invalid("checkpoint identity"));
        }
        Ok(Self {
            operation_id: checkpoint.operation_id,
            key_id: checkpoint.key_id,
            prefix: checkpoint.prefix,
            next_counter: checkpoint.next_counter,
        })
    }

    fn allocate_nonce(&mut self) -> Result<[u8; 12], CryptoContractError> {
        let counter = self
            .next_counter
            .ok_or(CryptoContractError::NonceExhausted)?;
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&self.prefix);
        nonce[4..].copy_from_slice(&counter.to_be_bytes());
        self.next_counter = counter.checked_add(1);
        Ok(nonce)
    }
}

trait PrivateStageAead {
    fn seal(
        &self,
        nonce: [u8; 12],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoContractError>;

    fn open(
        &self,
        nonce: [u8; 12],
        aad: &[u8],
        sealed: &[u8],
    ) -> Result<Vec<u8>, CryptoContractError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RecordIdentity {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    kind: StageKind,
    segment_id: u64,
    sequence: u64,
}

fn encode_aad(identity: RecordIdentity, plaintext_len: u64) -> [u8; AAD_LEN] {
    let mut aad = [0u8; AAD_LEN];
    aad[..8].copy_from_slice(RECORD_MAGIC);
    aad[8] = RECORD_VERSION;
    aad[9] = identity.kind as u8;
    aad[16..32].copy_from_slice(&identity.operation_id);
    aad[32..48].copy_from_slice(&identity.key_id);
    aad[48..56].copy_from_slice(&identity.segment_id.to_le_bytes());
    aad[56..64].copy_from_slice(&identity.sequence.to_le_bytes());
    aad[64..72].copy_from_slice(&plaintext_len.to_le_bytes());
    aad
}

fn encode_header(
    identity: RecordIdentity,
    plaintext_len: u64,
    nonce: [u8; 12],
) -> [u8; HEADER_LEN] {
    let mut header = [0u8; HEADER_LEN];
    header[..AAD_LEN].copy_from_slice(&encode_aad(identity, plaintext_len));
    header[72..84].copy_from_slice(&nonce);
    header
}

fn u64_field(bytes: &[u8], offset: usize) -> Result<u64, CryptoContractError> {
    let field = bytes
        .get(offset..offset + 8)
        .ok_or(CryptoContractError::Invalid("integer field"))?;
    Ok(u64::from_le_bytes(field.try_into().map_err(|_| {
        CryptoContractError::Invalid("integer field")
    })?))
}

fn decode_header(record: &[u8]) -> Result<(RecordIdentity, u64, [u8; 12]), CryptoContractError> {
    let header = record
        .get(..HEADER_LEN)
        .ok_or(CryptoContractError::Invalid("record header"))?;
    if &header[..8] != RECORD_MAGIC {
        return Err(CryptoContractError::Invalid("record magic"));
    }
    if header[8] != RECORD_VERSION {
        return Err(CryptoContractError::Invalid("record version"));
    }
    if header[10..16].iter().any(|byte| *byte != 0) || header[84..96].iter().any(|byte| *byte != 0)
    {
        return Err(CryptoContractError::Invalid("reserved bytes"));
    }
    let operation_id: [u8; 16] = header[16..32]
        .try_into()
        .map_err(|_| CryptoContractError::Invalid("operation id"))?;
    let key_id: [u8; 16] = header[32..48]
        .try_into()
        .map_err(|_| CryptoContractError::Invalid("key id"))?;
    if operation_id == [0; 16] || key_id == [0; 16] {
        return Err(CryptoContractError::Invalid("record identity"));
    }
    let nonce = header[72..84]
        .try_into()
        .map_err(|_| CryptoContractError::Invalid("nonce"))?;
    Ok((
        RecordIdentity {
            operation_id,
            key_id,
            kind: StageKind::decode(header[9])?,
            segment_id: u64_field(header, 48)?,
            sequence: u64_field(header, 56)?,
        },
        u64_field(header, 64)?,
        nonce,
    ))
}

fn seal_record<A: PrivateStageAead>(
    context: &mut OperationCryptoContext,
    kind: StageKind,
    segment_id: u64,
    sequence: u64,
    plaintext: &[u8],
    aead: &A,
) -> Result<Vec<u8>, CryptoContractError> {
    let plaintext_len =
        u64::try_from(plaintext.len()).map_err(|_| CryptoContractError::Invalid("length"))?;
    let identity = RecordIdentity {
        operation_id: context.operation_id,
        key_id: context.key_id,
        kind,
        segment_id,
        sequence,
    };
    let nonce = context.allocate_nonce()?;
    let aad = encode_aad(identity, plaintext_len);
    let sealed = aead.seal(nonce, &aad, plaintext)?;
    let mut record = Vec::with_capacity(HEADER_LEN + sealed.len());
    record.extend_from_slice(&encode_header(identity, plaintext_len, nonce));
    record.extend_from_slice(&sealed);
    Ok(record)
}

fn open_record<A: PrivateStageAead>(
    record: &[u8],
    expected: RecordIdentity,
    aead: &A,
) -> Result<Vec<u8>, CryptoContractError> {
    let (actual, plaintext_len, nonce) = decode_header(record)?;
    if actual != expected {
        return Err(CryptoContractError::AuthenticationFailed);
    }
    let sealed = record
        .get(HEADER_LEN..)
        .ok_or(CryptoContractError::Invalid("sealed body"))?;
    let plaintext = aead.open(nonce, &encode_aad(actual, plaintext_len), sealed)?;
    if u64::try_from(plaintext.len()).map_err(|_| CryptoContractError::Invalid("opened length"))?
        != plaintext_len
    {
        return Err(CryptoContractError::AuthenticationFailed);
    }
    Ok(plaintext)
}

/// Plumbing-only test adapter. Plaintext remains visible by design, so this
/// provides no confidentiality claim. Its digest suffix only detects incorrect
/// nonce/AAD/record wiring in the contract tests.
struct TestAuthenticatedPassthrough {
    test_key: [u8; 32],
}

impl TestAuthenticatedPassthrough {
    fn tag(&self, nonce: [u8; 12], aad: &[u8], plaintext: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"UCOF-TEST-ONLY-STAGE-AUTH\0");
        hasher.update(self.test_key);
        hasher.update(nonce);
        hasher.update(aad);
        hasher.update(plaintext);
        hasher.finalize().into()
    }
}

impl PrivateStageAead for TestAuthenticatedPassthrough {
    fn seal(
        &self,
        nonce: [u8; 12],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoContractError> {
        let mut sealed = plaintext.to_vec();
        sealed.extend_from_slice(&self.tag(nonce, aad, plaintext));
        Ok(sealed)
    }

    fn open(
        &self,
        nonce: [u8; 12],
        aad: &[u8],
        sealed: &[u8],
    ) -> Result<Vec<u8>, CryptoContractError> {
        let plaintext_len = sealed
            .len()
            .checked_sub(TEST_TAG_LEN)
            .ok_or(CryptoContractError::AuthenticationFailed)?;
        let (plaintext, tag) = sealed.split_at(plaintext_len);
        if tag != self.tag(nonce, aad, plaintext) {
            return Err(CryptoContractError::AuthenticationFailed);
        }
        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(prefix: [u8; 4]) -> OperationCryptoContext {
        OperationCryptoContext::new([0x11; 16], [0x22; 16], prefix).expect("context")
    }

    fn aead() -> TestAuthenticatedPassthrough {
        TestAuthenticatedPassthrough {
            test_key: [0x33; 32],
        }
    }

    fn identity(kind: StageKind, segment_id: u64, sequence: u64) -> RecordIdentity {
        RecordIdentity {
            operation_id: [0x11; 16],
            key_id: [0x22; 16],
            kind,
            segment_id,
            sequence,
        }
    }

    #[test]
    fn envelope_round_trip_binds_all_identity_fields_and_length() {
        let plaintext = b"descriptor-record";
        let expected = identity(StageKind::Descriptor, 7, 9);
        let aead = aead();
        let mut context = context([1, 2, 3, 4]);
        let record = seal_record(
            &mut context,
            expected.kind,
            expected.segment_id,
            expected.sequence,
            plaintext,
            &aead,
        )
        .expect("seal");

        assert_eq!(
            &record[HEADER_LEN..HEADER_LEN + plaintext.len()],
            plaintext,
            "test adapter must remain visibly non-confidential"
        );
        assert_eq!(
            open_record(&record, expected, &aead).expect("open"),
            plaintext
        );

        let mut wrong = expected;
        wrong.operation_id = [0x44; 16];
        assert_eq!(
            open_record(&record, wrong, &aead).expect_err("operation substitution"),
            CryptoContractError::AuthenticationFailed
        );
        wrong = expected;
        wrong.key_id = [0x55; 16];
        assert_eq!(
            open_record(&record, wrong, &aead).expect_err("key substitution"),
            CryptoContractError::AuthenticationFailed
        );
        wrong = expected;
        wrong.segment_id += 1;
        assert_eq!(
            open_record(&record, wrong, &aead).expect_err("segment substitution"),
            CryptoContractError::AuthenticationFailed
        );
        wrong = expected;
        wrong.kind = StageKind::Locator;
        assert_eq!(
            open_record(&record, wrong, &aead).expect_err("kind substitution"),
            CryptoContractError::AuthenticationFailed
        );
    }

    #[test]
    fn corruption_and_truncation_fail_closed() {
        let expected = identity(StageKind::PageRef, 4, 12);
        let aead = aead();
        let mut context = context([5, 6, 7, 8]);
        let record = seal_record(
            &mut context,
            expected.kind,
            expected.segment_id,
            expected.sequence,
            b"page-ref-record",
            &aead,
        )
        .expect("seal");

        let mut payload_corrupt = record.clone();
        payload_corrupt[HEADER_LEN] ^= 0x80;
        assert!(open_record(&payload_corrupt, expected, &aead).is_err());

        let mut tag_corrupt = record.clone();
        *tag_corrupt.last_mut().expect("tag byte") ^= 0x01;
        assert!(open_record(&tag_corrupt, expected, &aead).is_err());

        let mut length_substitution = record.clone();
        length_substitution[64] ^= 0x01;
        assert!(open_record(&length_substitution, expected, &aead).is_err());

        let mut reserved_corrupt = record.clone();
        reserved_corrupt[90] = 1;
        assert_eq!(
            open_record(&reserved_corrupt, expected, &aead).expect_err("reserved bytes"),
            CryptoContractError::Invalid("reserved bytes")
        );
        assert!(open_record(&record[..record.len() - 1], expected, &aead).is_err());
        assert!(open_record(&record[..HEADER_LEN - 1], expected, &aead).is_err());
    }

    #[test]
    fn reorder_and_duplicate_records_require_expected_sequence() {
        let aead = aead();
        let mut context = context([9, 10, 11, 12]);
        let first =
            seal_record(&mut context, StageKind::SortRun, 17, 0, b"first", &aead).expect("first");
        let second =
            seal_record(&mut context, StageKind::SortRun, 17, 1, b"second", &aead).expect("second");

        assert!(open_record(&second, identity(StageKind::SortRun, 17, 0), &aead).is_err());
        assert!(open_record(&first, identity(StageKind::SortRun, 17, 1), &aead).is_err());
        assert_eq!(
            open_record(&first, identity(StageKind::SortRun, 17, 0), &aead).expect("first open"),
            b"first"
        );
        assert_eq!(
            open_record(&second, identity(StageKind::SortRun, 17, 1), &aead).expect("second open"),
            b"second"
        );
    }

    #[test]
    fn million_nonce_allocations_are_monotonic_and_unique_by_construction() {
        let prefix = [0xa1, 0xb2, 0xc3, 0xd4];
        let mut context = context(prefix);
        for counter in 0u64..1_000_000 {
            let nonce = context.allocate_nonce().expect("nonce");
            assert_eq!(&nonce[..4], &prefix);
            assert_eq!(u64::from_be_bytes(nonce[4..].try_into().unwrap()), counter);
        }
        assert_eq!(context.checkpoint().next_counter, Some(1_000_000));
    }

    #[test]
    fn nonce_exhaustion_never_wraps() {
        let mut context = OperationCryptoContext::from_counter_for_test(
            [0x11; 16],
            [0x22; 16],
            [0xff; 4],
            u64::MAX - 1,
        )
        .expect("context");
        let penultimate = context.allocate_nonce().expect("penultimate");
        let final_nonce = context.allocate_nonce().expect("final");
        assert_eq!(
            u64::from_be_bytes(penultimate[4..].try_into().unwrap()),
            u64::MAX - 1
        );
        assert_eq!(
            u64::from_be_bytes(final_nonce[4..].try_into().unwrap()),
            u64::MAX
        );
        assert_eq!(
            context.allocate_nonce().expect_err("must not wrap"),
            CryptoContractError::NonceExhausted
        );
    }

    #[test]
    fn authenticated_resume_continues_global_nonce_counter() {
        let mut context = context([1, 1, 2, 3]);
        for expected in 0u64..3 {
            let nonce = context.allocate_nonce().expect("nonce");
            assert_eq!(u64::from_be_bytes(nonce[4..].try_into().unwrap()), expected);
        }
        let checkpoint = context.checkpoint();
        assert_eq!(
            OperationCryptoContext::resume(checkpoint, false)
                .expect_err("unauthenticated checkpoint"),
            CryptoContractError::UnauthenticatedCheckpoint
        );
        let mut resumed = OperationCryptoContext::resume(checkpoint, true).expect("resume");
        let nonce = resumed.allocate_nonce().expect("resumed nonce");
        assert_eq!(u64::from_be_bytes(nonce[4..].try_into().unwrap()), 3);
    }

    #[test]
    fn randomized_private_nonce_does_not_change_recovered_plaintext() {
        let plaintext = b"canonical-private-payload";
        let expected = identity(StageKind::Descriptor, 2, 5);
        let aead = aead();
        let mut first_context = context([1, 2, 3, 4]);
        let mut second_context = context([4, 3, 2, 1]);
        let first = seal_record(
            &mut first_context,
            expected.kind,
            expected.segment_id,
            expected.sequence,
            plaintext,
            &aead,
        )
        .expect("first seal");
        let second = seal_record(
            &mut second_context,
            expected.kind,
            expected.segment_id,
            expected.sequence,
            plaintext,
            &aead,
        )
        .expect("second seal");

        assert_ne!(first, second);
        assert_eq!(open_record(&first, expected, &aead).unwrap(), plaintext);
        assert_eq!(open_record(&second, expected, &aead).unwrap(), plaintext);
    }
}
