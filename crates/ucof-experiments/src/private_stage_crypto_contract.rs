//! Test-only contract model for authenticated private-stage records.
//!
//! This module deliberately does not provide production encryption. The
//! `PrivateStageAead` trait is the boundary for a vetted AEAD implementation.
//! Tests use an authenticated passthrough that leaves plaintext visible so the
//! research evidence cannot be mistaken for a confidentiality claim.

use sha2::{Digest, Sha256};
use std::fmt;

const PRIVATE_RECORD_MAGIC: &[u8; 8] = b"UCOFSTG1";
const PRIVATE_RECORD_VERSION: u8 = 1;
const PRIVATE_RECORD_HEADER_LEN: usize = 96;
const PRIVATE_RECORD_AAD_LEN: usize = 72;
const TEST_TAG_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrivateStageKind {
    SortRun = 1,
    Descriptor = 2,
    Locator = 3,
    PageRef = 4,
    Journal = 5,
}

impl PrivateStageKind {
    fn from_byte(value: u8) -> Result<Self, PrivateStageCryptoError> {
        match value {
            1 => Ok(Self::SortRun),
            2 => Ok(Self::Descriptor),
            3 => Ok(Self::Locator),
            4 => Ok(Self::PageRef),
            5 => Ok(Self::Journal),
            _ => Err(PrivateStageCryptoError::Invalid("stage kind")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PrivateStageCryptoError {
    Invalid(&'static str),
    NonceExhausted,
    UnauthenticatedCheckpoint,
    AuthenticationFailed,
    Crypto(&'static str),
}

impl fmt::Display for PrivateStageCryptoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(label) => write!(formatter, "invalid {label}"),
            Self::NonceExhausted => write!(formatter, "private-stage nonce space exhausted"),
            Self::UnauthenticatedCheckpoint => {
                write!(formatter, "private-stage nonce checkpoint is not authenticated")
            }
            Self::AuthenticationFailed => write!(formatter, "private-stage authentication failed"),
            Self::Crypto(label) => write!(formatter, "private-stage crypto failure: {label}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrivateNonceCheckpoint {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    next_counter: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PrivateOperationCryptoContext {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    next_counter: Option<u64>,
}

impl PrivateOperationCryptoContext {
    fn new(
        operation_id: [u8; 16],
        key_id: [u8; 16],
        nonce_prefix: [u8; 4],
    ) -> Result<Self, PrivateStageCryptoError> {
        if operation_id == [0; 16] {
            return Err(PrivateStageCryptoError::Invalid("operation id"));
        }
        if key_id == [0; 16] {
            return Err(PrivateStageCryptoError::Invalid("key id"));
        }
        Ok(Self {
            operation_id,
            key_id,
            nonce_prefix,
            next_counter: Some(0),
        })
    }

    fn from_counter_for_test(
        operation_id: [u8; 16],
        key_id: [u8; 16],
        nonce_prefix: [u8; 4],
        next_counter: u64,
    ) -> Result<Self, PrivateStageCryptoError> {
        let mut context = Self::new(operation_id, key_id, nonce_prefix)?;
        context.next_counter = Some(next_counter);
        Ok(context)
    }

    fn resume(
        checkpoint: PrivateNonceCheckpoint,
        authenticated: bool,
    ) -> Result<Self, PrivateStageCryptoError> {
        if !authenticated {
            return Err(PrivateStageCryptoError::UnauthenticatedCheckpoint);
        }
        if checkpoint.operation_id == [0; 16] || checkpoint.key_id == [0; 16] {
            return Err(PrivateStageCryptoError::Invalid("nonce checkpoint identity"));
        }
        Ok(Self {
            operation_id: checkpoint.operation_id,
            key_id: checkpoint.key_id,
            nonce_prefix: checkpoint.nonce_prefix,
            next_counter: checkpoint.next_counter,
        })
    }

    fn checkpoint(&self) -> PrivateNonceCheckpoint {
        PrivateNonceCheckpoint {
            operation_id: self.operation_id,
            key_id: self.key_id,
            nonce_prefix: self.nonce_prefix,
            next_counter: self.next_counter,
        }
    }

    fn allocate_nonce(&mut self) -> Result<[u8; 12], PrivateStageCryptoError> {
        let counter = self
            .next_counter
            .ok_or(PrivateStageCryptoError::NonceExhausted)?;
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&self.nonce_prefix);
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
    ) -> Result<Vec<u8>, PrivateStageCryptoError>;

    fn open(
        &self,
        nonce: [u8; 12],
        aad: &[u8],
        sealed: &[u8],
    ) -> Result<Vec<u8>, PrivateStageCryptoError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrivateRecordIdentity {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    kind: PrivateStageKind,
    segment_id: u64,
    sequence: u64,
}

fn encode_aad(
    identity: PrivateRecordIdentity,
    plaintext_len: u64,
) -> [u8; PRIVATE_RECORD_AAD_LEN] {
    let mut aad = [0u8; PRIVATE_RECORD_AAD_LEN];
    aad[..8].copy_from_slice(PRIVATE_RECORD_MAGIC);
    aad[8] = PRIVATE_RECORD_VERSION;
    aad[9] = identity.kind as u8;
    aad[16..32].copy_from_slice(&identity.operation_id);
    aad[32..48].copy_from_slice(&identity.key_id);
    aad[48..56].copy_from_slice(&identity.segment_id.to_le_bytes());
    aad[56..64].copy_from_slice(&identity.sequence.to_le_bytes());
    aad[64..72].copy_from_slice(&plaintext_len.to_le_bytes());
    aad
}

fn encode_header(
    identity: PrivateRecordIdentity,
    plaintext_len: u64,
    nonce: [u8; 12],
) -> [u8; PRIVATE_RECORD_HEADER_LEN] {
    let aad = encode_aad(identity, plaintext_len);
    let mut header = [0u8; PRIVATE_RECORD_HEADER_LEN];
    header[..PRIVATE_RECORD_AAD_LEN].copy_from_slice(&aad);
    header[72..84].copy_from_slice(&nonce);
    header
}

fn decode_u64(bytes: &[u8], offset: usize) -> Result<u64, PrivateStageCryptoError> {
    let end = offset
        .checked_add(8)
        .ok_or(PrivateStageCryptoError::Invalid("integer field"))?;
    let field = bytes
        .get(offset..end)
        .ok_or(PrivateStageCryptoError::Invalid("integer field"))?;
    Ok(u64::from_le_bytes(
        field
            .try_into()
            .map_err(|_| PrivateStageCryptoError::Invalid("integer field"))?,
    ))
}

fn decode_header(
    record: &[u8],
) -> Result<(PrivateRecordIdentity, u64, [u8; 12]), PrivateStageCryptoError> {
    let header = record
        .get(..PRIVATE_RECORD_HEADER_LEN)
        .ok_or(PrivateStageCryptoError::Invalid("record header"))?;
    if &header[..8] != PRIVATE_RECORD_MAGIC {
        return Err(PrivateStageCryptoError::Invalid("record magic"));
    }
    if header[8] != PRIVATE_RECORD_VERSION {
        return Err(PrivateStageCryptoError::Invalid("record version"));
    }
    if header[10..16].iter().any(|byte| *byte != 0)
        || header[84..96].iter().any(|byte| *byte != 0)
    {
        return Err(PrivateStageCryptoError::Invalid("record reserved bytes"));
    }
    let operation_id: [u8; 16] = header[16..32]
        .try_into()
        .map_err(|_| PrivateStageCryptoError::Invalid("operation id"))?;
    let key_id: [u8; 16] = header[32..48]
        .try_into()
        .map_err(|_| PrivateStageCryptoError::Invalid("key id"))?;
    if operation_id == [0; 16] || key_id == [0; 16] {
        return Err(PrivateStageCryptoError::Invalid("record identity"));
    }
    let nonce = header[72..84]
        .try_into()
        .map_err(|_| PrivateStageCryptoError::Invalid("nonce"))?;
    Ok((
        PrivateRecordIdentity {
            operation_id,
            key_id,
            kind: PrivateStageKind::from_byte(header[9])?,
            segment_id: decode_u64(header, 48)?,
            sequence: decode_u64(header, 56)?,
        },
        decode_u64(header, 64)?,
        nonce,
    ))
}

fn seal_private_record<A: PrivateStageAead>(
    context: &mut PrivateOperationCryptoContext,
    kind: PrivateStageKind,
    segment_id: u64,
    sequence: u64,
    plaintext: &[u8],
    aead: &A,
) -> Result<Vec<u8>, PrivateStageCryptoError> {
    let plaintext_len = u64::try_from(plaintext.len())
        .map_err(|_| PrivateStageCryptoError::Invalid("plaintext length"))?;
    let identity = PrivateRecordIdentity {
        operation_id: context.operation_id,
        key_id: context.key_id,
        kind,
        segment_id,
        sequence,
    };
    let nonce = context.allocate_nonce()?;
    let aad = encode_aad(identity, plaintext_len);
    let sealed = aead.seal(nonce, &aad, plaintext)?;
    let mut record = Vec::with_capacity(
        PRIVATE_RECORD_HEADER_LEN
            .checked_add(sealed.len())
            .ok_or(PrivateStageCryptoError::Invalid("sealed record length"))?,
    );
    record.extend_from_slice(&encode_header(identity, plaintext_len, nonce));
    record.extend_from_slice(&sealed);
    Ok(record)
}

fn open_private_record<A: PrivateStageAead>(
    record: &[u8],
    expected: PrivateRecordIdentity,
    aead: &A,
) -> Result<Vec<u8>, PrivateStageCryptoError> {
    let (actual, plaintext_len, nonce) = decode_header(record)?;
    if actual != expected {
        return Err(PrivateStageCryptoError::AuthenticationFailed);
    }
    let aad = encode_aad(actual, plaintext_len);
    let sealed = record
        .get(PRIVATE_RECORD_HEADER_LEN..)
        .ok_or(PrivateStageCryptoError::Invalid("sealed body"))?;
    let plaintext = aead.open(nonce, &aad, sealed)?;
    if u64::try_from(plaintext.len())
        .map_err(|_| PrivateStageCryptoError::Invalid("opened length"))?
        != plaintext_len
    {
        return Err(PrivateStageCryptoError::AuthenticationFailed);
    }
    Ok(plaintext)
}

/// Plumbing-only test double. It intentionally leaves plaintext visible and
/// therefore provides no confidentiality claim. The SHA-256 suffix is only a
/// deterministic test authenticator for detecting incorrect nonce/AAD wiring.
struct TestOnlyAuthenticatedPassthrough {
    test_key: [u8; 32],
}

impl TestOnlyAuthenticatedPassthrough {
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

impl PrivateStageAead for TestOnlyAuthenticatedPassthrough {
    fn seal(
        &self,
        nonce: [u8; 12],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, PrivateStageCryptoError> {
        let mut sealed = plaintext.to_vec();
        sealed.extend_from_slice(&self.tag(nonce, aad, plaintext));
        Ok(sealed)
    }

    fn open(
        &self,
        nonce: [u8; 12],
        aad: &[u8],
        sealed: &[u8],
    ) -> Result<Vec<u8>, PrivateStageCryptoError> {
        let plaintext_len = sealed
            .len()
            .checked_sub(TEST_TAG_LEN)
            .ok_or(PrivateStageCryptoError::AuthenticationFailed)?;
        let (plaintext, tag) = sealed.split_at(plaintext_len);
        if tag != self.tag(nonce, aad, plaintext) {
            return Err(PrivateStageCryptoError::AuthenticationFailed);
        }
        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(prefix: [u8; 4]) -> PrivateOperationCryptoContext {
        PrivateOperationCryptoContext::new([0x11; 16], [0x22; 16], prefix).expect("context")
    }

    fn aead() -> TestOnlyAuthenticatedPassthrough {
        TestOnlyAuthenticatedPassthrough {
            test_key: [0x33; 32],
        }
    }

    fn identity(kind: PrivateStageKind, segment_id: u64, sequence: u64) -> PrivateRecordIdentity {
        PrivateRecordIdentity {
            operation_id: [0x11; 16],
            key_id: [0x22; 16],
            kind,
            segment_id,
            sequence,
        }
    }

    #[test]
    fn envelope_round_trip_binds_operation_key_segment_sequence_and_length() {
        let plaintext = b"descriptor-record";
        let mut context = context([1, 2, 3, 4]);
        let aead = aead();
        let record = seal_private_record(
            &mut context,
            PrivateStageKind::Descriptor,
            7,
            9,
            plaintext,
            &aead,
        )
        .expect("seal");

        assert_eq!(
            &record[PRIVATE_RECORD_HEADER_LEN..PRIVATE_RECORD_HEADER_LEN + plaintext.len()],
            plaintext,
            "test double must remain visibly non-confidential"
        );
        assert_eq!(
            open_private_record(
                &record,
                identity(PrivateStageKind::Descriptor, 7, 9),
                &aead,
            )
            .expect("open"),
            plaintext
        );
        assert_eq!(
            open_private_record(
                &record,
                identity(PrivateStageKind::Descriptor, 8, 9),
                &aead,
            )
            .expect_err("segment substitution"),
            PrivateStageCryptoError::AuthenticationFailed
        );
        assert_eq!(
            open_private_record(
                &record,
                identity(PrivateStageKind::Locator, 7, 9),
                &aead,
            )
            .expect_err("kind substitution"),
            PrivateStageCryptoError::AuthenticationFailed
        );
    }

    #[test]
    fn corruption_substitution_and_truncation_fail_closed() {
        let mut context = context([5, 6, 7, 8]);
        let aead = aead();
        let expected = identity(PrivateStageKind::PageRef, 4, 12);
        let record = seal_private_record(
            &mut context,
            expected.kind,
            expected.segment_id,
            expected.sequence,
            b"page-ref-record",
            &aead,
        )
        .expect("seal");

        let mut payload_corrupt = record.clone();
        payload_corrupt[PRIVATE_RECORD_HEADER_LEN] ^= 0x80;
        assert!(open_private_record(&payload_corrupt, expected, &aead).is_err());

        let mut tag_corrupt = record.clone();
        *tag_corrupt.last_mut().expect("tag byte") ^= 0x01;
        assert!(open_private_record(&tag_corrupt, expected, &aead).is_err());

        let mut length_substitution = record.clone();
        length_substitution[64] ^= 0x01;
        assert!(open_private_record(&length_substitution, expected, &aead).is_err());

        let mut reserved_corrupt = record.clone();
        reserved_corrupt[90] = 1;
        assert_eq!(
            open_private_record(&reserved_corrupt, expected, &aead)
                .expect_err("reserved bytes"),
            PrivateStageCryptoError::Invalid("record reserved bytes")
        );

        assert!(open_private_record(&record[..record.len() - 1], expected, &aead).is_err());
        assert!(open_private_record(&record[..PRIVATE_RECORD_HEADER_LEN - 1], expected, &aead)
            .is_err());
    }

    #[test]
    fn reorder_and_duplicate_records_require_expected_sequence() {
        let mut context = context([9, 10, 11, 12]);
        let aead = aead();
        let first = seal_private_record(
            &mut context,
            PrivateStageKind::SortRun,
            17,
            0,
            b"first",
            &aead,
        )
        .expect("first");
        let second = seal_private_record(
            &mut context,
            PrivateStageKind::SortRun,
            17,
            1,
            b"second",
            &aead,
        )
        .expect("second");

        assert!(open_private_record(
            &second,
            identity(PrivateStageKind::SortRun, 17, 0),
            &aead,
        )
        .is_err());
        assert!(open_private_record(
            &first,
            identity(PrivateStageKind::SortRun, 17, 1),
            &aead,
        )
        .is_err());
        assert_eq!(
            open_private_record(
                &first,
                identity(PrivateStageKind::SortRun, 17, 0),
                &aead,
            )
            .expect("ordered first"),
            b"first"
        );
        assert_eq!(
            open_private_record(
                &second,
                identity(PrivateStageKind::SortRun, 17, 1),
                &aead,
            )
            .expect("ordered second"),
            b"second"
        );
    }

    #[test]
    fn million_nonce_allocations_are_monotonic_and_unique_without_storage() {
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
        let mut context = PrivateOperationCryptoContext::from_counter_for_test(
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
            PrivateStageCryptoError::NonceExhausted
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
            PrivateOperationCryptoContext::resume(checkpoint, false)
                .expect_err("unauthenticated checkpoint must fail"),
            PrivateStageCryptoError::UnauthenticatedCheckpoint
        );
        let mut resumed =
            PrivateOperationCryptoContext::resume(checkpoint, true).expect("authenticated resume");
        let nonce = resumed.allocate_nonce().expect("resumed nonce");
        assert_eq!(u64::from_be_bytes(nonce[4..].try_into().unwrap()), 3);
    }

    #[test]
    fn different_nonce_prefixes_change_private_record_but_not_recovered_plaintext() {
        let aead = aead();
        let plaintext = b"canonical-private-payload";
        let expected = identity(PrivateStageKind::Descriptor, 2, 5);
        let mut first_context = context([1, 2, 3, 4]);
        let mut second_context = context([4, 3, 2, 1]);
        let first = seal_private_record(
            &mut first_context,
            expected.kind,
            expected.segment_id,
            expected.sequence,
            plaintext,
            &aead,
        )
        .expect("first seal");
        let second = seal_private_record(
            &mut second_context,
            expected.kind,
            expected.segment_id,
            expected.sequence,
            plaintext,
            &aead,
        )
        .expect("second seal");

        assert_ne!(first, second);
        assert_eq!(open_private_record(&first, expected, &aead).unwrap(), plaintext);
        assert_eq!(open_private_record(&second, expected, &aead).unwrap(), plaintext);
    }

    #[test]
    fn test_authenticator_crypto_error_variant_remains_available_for_real_adapter() {
        assert_eq!(
            PrivateStageCryptoError::Crypto("adapter"),
            PrivateStageCryptoError::Crypto("adapter")
        );
    }
}
