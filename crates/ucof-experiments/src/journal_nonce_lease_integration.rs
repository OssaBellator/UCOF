//! Test-only integration contract between authenticated journal generations and nonce leases.

use sha2::{Digest, Sha256};

const TAG_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DurableJournal {
    operation_id: [u8; 16],
    key_id: [u8; 16],
    generation: u64,
    next_unreserved: Option<u64>,
}

impl DurableJournal {
    fn initial() -> Self {
        Self {
            operation_id: [0x11; 16],
            key_id: [0x22; 16],
            generation: 0,
            next_unreserved: Some(0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingLease {
    base: DurableJournal,
    candidate: DurableJournal,
    first: u64,
    last: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveLease {
    committed_generation: u64,
    next: Option<u64>,
    last: u64,
}

impl ActiveLease {
    fn allocate(&mut self) -> Result<u64, IntegrationError> {
        let counter = self.next.ok_or(IntegrationError::LeaseExhausted)?;
        if counter > self.last {
            self.next = None;
            return Err(IntegrationError::LeaseExhausted);
        }
        self.next = if counter == self.last {
            None
        } else {
            Some(counter + 1)
        };
        Ok(counter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntegrationError {
    InvalidLeaseSize,
    CounterExhausted,
    CounterOverflow,
    GenerationExhausted,
    AuthenticationFailed,
    NotDurablyCommitted,
    StaleBase,
    CandidateMismatch,
    LeaseExhausted,
}

fn reserve_lease(
    durable: DurableJournal,
    lease_size: u64,
    max_lease_size: u64,
) -> Result<PendingLease, IntegrationError> {
    if lease_size == 0 || max_lease_size == 0 || lease_size > max_lease_size {
        return Err(IntegrationError::InvalidLeaseSize);
    }
    let first = durable
        .next_unreserved
        .ok_or(IntegrationError::CounterExhausted)?;
    let last = first
        .checked_add(lease_size - 1)
        .ok_or(IntegrationError::CounterOverflow)?;
    let candidate = DurableJournal {
        operation_id: durable.operation_id,
        key_id: durable.key_id,
        generation: durable
            .generation
            .checked_add(1)
            .ok_or(IntegrationError::GenerationExhausted)?,
        next_unreserved: last.checked_add(1),
    };
    Ok(PendingLease {
        base: durable,
        candidate,
        first,
        last,
    })
}

fn encode_journal(journal: DurableJournal) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    bytes[..16].copy_from_slice(&journal.operation_id);
    bytes[16..32].copy_from_slice(&journal.key_id);
    bytes[32..40].copy_from_slice(&journal.generation.to_le_bytes());
    bytes[40] = u8::from(journal.next_unreserved.is_some());
    if let Some(counter) = journal.next_unreserved {
        bytes[48..56].copy_from_slice(&counter.to_le_bytes());
    }
    bytes
}

fn decode_journal(bytes: &[u8]) -> Result<DurableJournal, IntegrationError> {
    if bytes.len() != 64 || bytes[41..48].iter().any(|byte| *byte != 0) || bytes[56..].iter().any(|byte| *byte != 0) {
        return Err(IntegrationError::CandidateMismatch);
    }
    if bytes[40] > 1 {
        return Err(IntegrationError::CandidateMismatch);
    }
    let operation_id: [u8; 16] = bytes[..16]
        .try_into()
        .map_err(|_| IntegrationError::CandidateMismatch)?;
    let key_id: [u8; 16] = bytes[16..32]
        .try_into()
        .map_err(|_| IntegrationError::CandidateMismatch)?;
    let generation = u64::from_le_bytes(
        bytes[32..40]
            .try_into()
            .map_err(|_| IntegrationError::CandidateMismatch)?,
    );
    let encoded_counter = u64::from_le_bytes(
        bytes[48..56]
            .try_into()
            .map_err(|_| IntegrationError::CandidateMismatch)?,
    );
    let next_unreserved = if bytes[40] == 1 {
        Some(encoded_counter)
    } else {
        if encoded_counter != 0 {
            return Err(IntegrationError::CandidateMismatch);
        }
        None
    };
    Ok(DurableJournal {
        operation_id,
        key_id,
        generation,
        next_unreserved,
    })
}

struct TestJournalAuth {
    key: [u8; 32],
}

impl TestJournalAuth {
    fn tag(&self, plaintext: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"UCOF-TEST-JOURNAL-LEASE-INTEGRATION\0");
        hasher.update(self.key);
        hasher.update(plaintext);
        hasher.finalize().into()
    }

    fn seal(&self, journal: DurableJournal) -> Vec<u8> {
        let plaintext = encode_journal(journal);
        let mut sealed = plaintext.to_vec();
        sealed.extend_from_slice(&self.tag(&plaintext));
        sealed
    }

    fn open(&self, sealed: &[u8]) -> Result<DurableJournal, IntegrationError> {
        let plaintext_len = sealed
            .len()
            .checked_sub(TAG_LEN)
            .ok_or(IntegrationError::AuthenticationFailed)?;
        let (plaintext, tag) = sealed.split_at(plaintext_len);
        if tag != self.tag(plaintext) {
            return Err(IntegrationError::AuthenticationFailed);
        }
        decode_journal(plaintext)
    }
}

fn activate_committed_lease(
    current: DurableJournal,
    pending: PendingLease,
    sealed_candidate: &[u8],
    authenticator: &TestJournalAuth,
    durably_committed: bool,
) -> Result<(DurableJournal, ActiveLease), IntegrationError> {
    if current != pending.base {
        return Err(IntegrationError::StaleBase);
    }
    if !durably_committed {
        return Err(IntegrationError::NotDurablyCommitted);
    }
    let authenticated = authenticator.open(sealed_candidate)?;
    if authenticated != pending.candidate {
        return Err(IntegrationError::CandidateMismatch);
    }
    Ok((
        authenticated,
        ActiveLease {
            committed_generation: authenticated.generation,
            next: Some(pending.first),
            last: pending.last,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> TestJournalAuth {
        TestJournalAuth { key: [0x33; 32] }
    }

    #[test]
    fn crash_before_durable_journal_commit_cannot_activate_nonce() {
        let durable = DurableJournal::initial();
        let pending = reserve_lease(durable, 4, 16).expect("reserve");
        let sealed = auth().seal(pending.candidate);
        assert_eq!(
            activate_committed_lease(durable, pending, &sealed, &auth(), false)
                .expect_err("not durable"),
            IntegrationError::NotDurablyCommitted
        );
        assert_eq!(durable.next_unreserved, Some(0));
    }

    #[test]
    fn crash_after_durable_commit_burns_entire_lease_at_every_use_cut() {
        for used_before_crash in 0usize..=4 {
            let durable = DurableJournal::initial();
            let pending = reserve_lease(durable, 4, 16).expect("reserve");
            let sealed = auth().seal(pending.candidate);
            let (committed, mut lease) =
                activate_committed_lease(durable, pending, &sealed, &auth(), true)
                    .expect("activate");
            let mut used = Vec::new();
            for _ in 0..used_before_crash {
                used.push(lease.allocate().expect("pre-crash nonce"));
            }

            assert_eq!(committed.next_unreserved, Some(4));
            let next_pending = reserve_lease(committed, 4, 16).expect("next reserve");
            let next_sealed = auth().seal(next_pending.candidate);
            let (next_committed, mut next_lease) = activate_committed_lease(
                committed,
                next_pending,
                &next_sealed,
                &auth(),
                true,
            )
            .expect("next activate");
            assert_eq!(next_committed.next_unreserved, Some(8));
            while let Ok(counter) = next_lease.allocate() {
                assert!(counter >= 4);
                assert!(!used.contains(&counter));
            }
        }
    }

    #[test]
    fn authenticated_candidate_generation_must_exactly_match_pending_lease() {
        let durable = DurableJournal::initial();
        let pending = reserve_lease(durable, 4, 16).expect("reserve");
        let foreign_candidate = DurableJournal {
            generation: pending.candidate.generation + 1,
            ..pending.candidate
        };
        let sealed = auth().seal(foreign_candidate);
        assert_eq!(
            activate_committed_lease(durable, pending, &sealed, &auth(), true)
                .expect_err("candidate mismatch"),
            IntegrationError::CandidateMismatch
        );
    }

    #[test]
    fn tampered_or_wrongly_authenticated_journal_cannot_activate_lease() {
        let durable = DurableJournal::initial();
        let pending = reserve_lease(durable, 4, 16).expect("reserve");
        let mut sealed = auth().seal(pending.candidate);
        sealed[32] ^= 1;
        assert_eq!(
            activate_committed_lease(durable, pending, &sealed, &auth(), true)
                .expect_err("tamper"),
            IntegrationError::AuthenticationFailed
        );

        let sealed = auth().seal(pending.candidate);
        let wrong = TestJournalAuth { key: [0x44; 32] };
        assert_eq!(
            activate_committed_lease(durable, pending, &sealed, &wrong, true)
                .expect_err("wrong auth key"),
            IntegrationError::AuthenticationFailed
        );
    }

    #[test]
    fn stale_pending_lease_cannot_activate_after_durable_generation_advances() {
        let durable = DurableJournal::initial();
        let stale = reserve_lease(durable, 4, 16).expect("stale reserve");
        let winning = reserve_lease(durable, 8, 16).expect("winning reserve");
        let winning_sealed = auth().seal(winning.candidate);
        let (advanced, _) = activate_committed_lease(
            durable,
            winning,
            &winning_sealed,
            &auth(),
            true,
        )
        .expect("winning activate");
        let stale_sealed = auth().seal(stale.candidate);
        assert_eq!(
            activate_committed_lease(advanced, stale, &stale_sealed, &auth(), true)
                .expect_err("stale base"),
            IntegrationError::StaleBase
        );
    }

    #[test]
    fn active_lease_records_exact_committed_generation() {
        let durable = DurableJournal::initial();
        let pending = reserve_lease(durable, 3, 16).expect("reserve");
        let sealed = auth().seal(pending.candidate);
        let (committed, lease) =
            activate_committed_lease(durable, pending, &sealed, &auth(), true)
                .expect("activate");
        assert_eq!(lease.committed_generation, committed.generation);
        assert_eq!(lease.committed_generation, 1);
    }
}
