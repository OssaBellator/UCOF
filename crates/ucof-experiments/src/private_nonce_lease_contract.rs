// Test-only crash-safety contract for private-stage nonce leases.
//
// The model requires a lease high-water mark to be durably committed before
// any nonce in the lease can be issued. Durability itself is an external
// boundary in this experiment; no filesystem journal implementation is
// claimed here.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DurableNonceState {
    generation: u64,
    next_unreserved: Option<u64>,
}

impl DurableNonceState {
    fn initial() -> Self {
        Self {
            generation: 0,
            next_unreserved: Some(0),
        }
    }

    fn from_counter_for_test(next_unreserved: u64) -> Self {
        Self {
            generation: 0,
            next_unreserved: Some(next_unreserved),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingNonceLease {
    base: DurableNonceState,
    committed: DurableNonceState,
    first: u64,
    last: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveNonceLease {
    first: u64,
    next: Option<u64>,
    last: u64,
}

impl ActiveNonceLease {
    fn allocate(&mut self) -> Result<u64, NonceLeaseError> {
        let counter = self.next.ok_or(NonceLeaseError::LeaseExhausted)?;
        if counter > self.last {
            self.next = None;
            return Err(NonceLeaseError::LeaseExhausted);
        }
        self.next = if counter == self.last {
            None
        } else {
            Some(counter + 1)
        };
        Ok(counter)
    }

    fn remaining(&self) -> u64 {
        match self.next {
            Some(next) => self.last - next + 1,
            None => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NonceLeaseError {
    InvalidLeaseSize,
    LeaseTooLarge,
    CounterExhausted,
    CounterOverflow,
    GenerationExhausted,
    StaleReservation,
    NotDurablyCommitted,
    LeaseExhausted,
}

fn reserve_nonce_lease(
    durable: DurableNonceState,
    lease_size: u64,
    max_lease_size: u64,
) -> Result<PendingNonceLease, NonceLeaseError> {
    if lease_size == 0 || max_lease_size == 0 {
        return Err(NonceLeaseError::InvalidLeaseSize);
    }
    if lease_size > max_lease_size {
        return Err(NonceLeaseError::LeaseTooLarge);
    }
    let first = durable
        .next_unreserved
        .ok_or(NonceLeaseError::CounterExhausted)?;
    let last = first
        .checked_add(lease_size - 1)
        .ok_or(NonceLeaseError::CounterOverflow)?;
    let committed = DurableNonceState {
        generation: durable
            .generation
            .checked_add(1)
            .ok_or(NonceLeaseError::GenerationExhausted)?,
        next_unreserved: last.checked_add(1),
    };
    Ok(PendingNonceLease {
        base: durable,
        committed,
        first,
        last,
    })
}

fn activate_nonce_lease(
    current_durable: DurableNonceState,
    pending: PendingNonceLease,
    durably_committed: bool,
) -> Result<(DurableNonceState, ActiveNonceLease), NonceLeaseError> {
    if current_durable != pending.base {
        return Err(NonceLeaseError::StaleReservation);
    }
    if !durably_committed {
        return Err(NonceLeaseError::NotDurablyCommitted);
    }
    Ok((
        pending.committed,
        ActiveNonceLease {
            first: pending.first,
            next: Some(pending.first),
            last: pending.last,
        },
    ))
}

fn restart_from_durable(durable: DurableNonceState) -> DurableNonceState {
    durable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed_lease(
        durable: DurableNonceState,
        lease_size: u64,
    ) -> (DurableNonceState, ActiveNonceLease) {
        let pending = reserve_nonce_lease(durable, lease_size, 1024).expect("reserve");
        activate_nonce_lease(durable, pending, true).expect("commit")
    }

    #[test]
    fn pending_lease_cannot_issue_nonce_before_durable_commit() {
        let durable = DurableNonceState::initial();
        let pending = reserve_nonce_lease(durable, 4, 16).expect("reserve");
        assert_eq!(pending.first, 0);
        assert_eq!(pending.last, 3);
        assert_eq!(pending.committed.next_unreserved, Some(4));
        assert_eq!(
            activate_nonce_lease(durable, pending, false).expect_err("not durable"),
            NonceLeaseError::NotDurablyCommitted
        );
        assert_eq!(restart_from_durable(durable).next_unreserved, Some(0));
    }

    #[test]
    fn every_crash_cut_after_durable_commit_abandons_unused_counters_without_reuse() {
        for used_before_crash in 0usize..=4 {
            let durable = DurableNonceState::initial();
            let (committed, mut lease) = committed_lease(durable, 4);
            let mut used = Vec::new();
            for _ in 0..used_before_crash {
                used.push(lease.allocate().expect("pre-crash nonce"));
            }

            let restarted = restart_from_durable(committed);
            assert_eq!(restarted.next_unreserved, Some(4));
            let (next_committed, mut next_lease) = committed_lease(restarted, 4);
            assert_eq!(next_committed.next_unreserved, Some(8));
            let mut after_restart = Vec::new();
            while let Ok(counter) = next_lease.allocate() {
                after_restart.push(counter);
            }

            assert!(used.iter().all(|counter| *counter < 4));
            assert!(after_restart.iter().all(|counter| *counter >= 4));
            assert!(used.iter().all(|counter| !after_restart.contains(counter)));
        }
    }

    #[test]
    fn crash_before_durable_commit_may_reuse_reserved_numbers_because_none_were_issuable() {
        let durable = DurableNonceState::initial();
        let first_pending = reserve_nonce_lease(durable, 4, 16).expect("reserve");
        assert_eq!(first_pending.first, 0);
        assert_eq!(first_pending.last, 3);

        let restarted = restart_from_durable(durable);
        let second_pending = reserve_nonce_lease(restarted, 4, 16).expect("reserve again");
        assert_eq!(second_pending.first, 0);
        assert_eq!(second_pending.last, 3);
        let (_, mut active) =
            activate_nonce_lease(restarted, second_pending, true).expect("commit second");
        assert_eq!(active.allocate().expect("first usable nonce"), 0);
    }

    #[test]
    fn sequential_committed_leases_are_disjoint_and_monotonic() {
        let mut durable = DurableNonceState::initial();
        for lease_index in 0u64..1000 {
            let (next_durable, mut lease) = committed_lease(durable, 17);
            assert_eq!(lease.first, lease_index * 17);
            assert_eq!(lease.last, lease_index * 17 + 16);
            assert_eq!(lease.remaining(), 17);
            for offset in 0u64..17 {
                assert_eq!(lease.allocate().expect("nonce"), lease_index * 17 + offset);
            }
            assert_eq!(lease.remaining(), 0);
            assert_eq!(
                lease.allocate().expect_err("lease exhausted"),
                NonceLeaseError::LeaseExhausted
            );
            durable = next_durable;
        }
        assert_eq!(durable.generation, 1000);
        assert_eq!(durable.next_unreserved, Some(17_000));
    }

    #[test]
    fn stale_pending_reservation_cannot_commit_after_another_lease_advances_state() {
        let durable = DurableNonceState::initial();
        let stale = reserve_nonce_lease(durable, 4, 16).expect("stale reserve");
        let winning = reserve_nonce_lease(durable, 8, 16).expect("winning reserve");
        let (advanced, _) = activate_nonce_lease(durable, winning, true).expect("winning commit");
        assert_eq!(advanced.next_unreserved, Some(8));
        assert_eq!(
            activate_nonce_lease(advanced, stale, true).expect_err("stale commit"),
            NonceLeaseError::StaleReservation
        );
    }

    #[test]
    fn lease_size_is_bounded_before_state_advance() {
        let durable = DurableNonceState::initial();
        assert_eq!(
            reserve_nonce_lease(durable, 0, 16).expect_err("zero size"),
            NonceLeaseError::InvalidLeaseSize
        );
        assert_eq!(
            reserve_nonce_lease(durable, 17, 16).expect_err("too large"),
            NonceLeaseError::LeaseTooLarge
        );
        assert_eq!(durable, DurableNonceState::initial());
    }

    #[test]
    fn final_counter_can_be_leased_once_and_exhaustion_never_wraps() {
        let durable = DurableNonceState::from_counter_for_test(u64::MAX - 1);
        let pending = reserve_nonce_lease(durable, 2, 2).expect("final lease");
        assert_eq!(pending.first, u64::MAX - 1);
        assert_eq!(pending.last, u64::MAX);
        assert_eq!(pending.committed.next_unreserved, None);
        let (exhausted, mut lease) =
            activate_nonce_lease(durable, pending, true).expect("commit final lease");
        assert_eq!(lease.allocate().expect("penultimate"), u64::MAX - 1);
        assert_eq!(lease.allocate().expect("final"), u64::MAX);
        assert_eq!(
            lease.allocate().expect_err("active exhausted"),
            NonceLeaseError::LeaseExhausted
        );
        assert_eq!(
            reserve_nonce_lease(exhausted, 1, 2).expect_err("global exhausted"),
            NonceLeaseError::CounterExhausted
        );
    }

    #[test]
    fn lease_reservation_never_wraps_counter_range() {
        let durable = DurableNonceState::from_counter_for_test(u64::MAX);
        assert_eq!(
            reserve_nonce_lease(durable, 2, 2).expect_err("range overflow"),
            NonceLeaseError::CounterOverflow
        );
        let pending = reserve_nonce_lease(durable, 1, 2).expect("single final nonce");
        assert_eq!(pending.last, u64::MAX);
        assert_eq!(pending.committed.next_unreserved, None);
    }

    #[test]
    fn generation_exhaustion_prevents_lease_activation() {
        let durable = DurableNonceState {
            generation: u64::MAX,
            next_unreserved: Some(0),
        };
        assert_eq!(
            reserve_nonce_lease(durable, 1, 1).expect_err("generation overflow"),
            NonceLeaseError::GenerationExhausted
        );
    }
}
