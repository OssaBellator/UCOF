/// Deterministic delay policy for retryable conditional-source transport failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionalBackoffPolicy {
    base_delay_millis: u64,
    max_delay_millis: u64,
    max_cumulative_delay_millis: u64,
}

impl ConditionalBackoffPolicy {
    pub fn new(
        base_delay_millis: u64,
        max_delay_millis: u64,
        max_cumulative_delay_millis: u64,
    ) -> Result<Self, ConditionalSourceError> {
        if base_delay_millis == 0
            || max_delay_millis < base_delay_millis
            || max_cumulative_delay_millis < base_delay_millis
        {
            return Err(ConditionalSourceError::Limit("retry delay configuration"));
        }
        Ok(Self {
            base_delay_millis,
            max_delay_millis,
            max_cumulative_delay_millis,
        })
    }

    #[must_use]
    pub fn base_delay_millis(self) -> u64 {
        self.base_delay_millis
    }

    #[must_use]
    pub fn max_delay_millis(self) -> u64 {
        self.max_delay_millis
    }

    #[must_use]
    pub fn max_cumulative_delay_millis(self) -> u64 {
        self.max_cumulative_delay_millis
    }
}

impl Default for ConditionalBackoffPolicy {
    fn default() -> Self {
        Self {
            base_delay_millis: 100,
            max_delay_millis: 5_000,
            max_cumulative_delay_millis: 30_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionalBackoffDecision {
    pub retry_index: u32,
    pub delay_millis: u64,
    pub cumulative_delay_millis: u64,
    pub used_server_minimum: bool,
}

/// Pure operation-wide delay budget for retryable conditional-source failures.
///
/// This planner performs no sleeping and introduces no jitter. A transport adapter must check its
/// cancellation/deadline control immediately before and after any real wait. A server-provided
/// minimum delay is accepted only when it fits the configured per-delay cap; silently truncating a
/// larger minimum could violate the server contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalBackoffBudget {
    policy: ConditionalBackoffPolicy,
    retries_planned: u32,
    cumulative_delay_millis: u64,
}

impl ConditionalBackoffBudget {
    #[must_use]
    pub fn new(policy: ConditionalBackoffPolicy) -> Self {
        Self {
            policy,
            retries_planned: 0,
            cumulative_delay_millis: 0,
        }
    }

    #[must_use]
    pub fn retries_planned(&self) -> u32 {
        self.retries_planned
    }

    #[must_use]
    pub fn cumulative_delay_millis(&self) -> u64 {
        self.cumulative_delay_millis
    }

    pub fn plan_next_delay(
        &mut self,
        server_minimum_millis: Option<u64>,
        remaining_deadline_millis: Option<u64>,
    ) -> Result<ConditionalBackoffDecision, ConditionalSourceError> {
        let exponential = self
            .policy
            .base_delay_millis
            .checked_shl(self.retries_planned.min(63))
            .unwrap_or(u64::MAX)
            .min(self.policy.max_delay_millis);
        let server_minimum = server_minimum_millis.unwrap_or(0);
        if server_minimum > self.policy.max_delay_millis {
            return Err(ConditionalSourceError::Limit("server retry delay"));
        }
        let delay_millis = exponential.max(server_minimum);
        if remaining_deadline_millis.is_some_and(|remaining| delay_millis >= remaining) {
            return Err(ConditionalSourceError::DeadlineExceeded);
        }
        let cumulative_delay_millis = self
            .cumulative_delay_millis
            .checked_add(delay_millis)
            .ok_or(ConditionalSourceError::Limit("retry delay"))?;
        if cumulative_delay_millis > self.policy.max_cumulative_delay_millis {
            return Err(ConditionalSourceError::Limit("retry delay"));
        }
        let retry_index = self.retries_planned;
        self.retries_planned = self
            .retries_planned
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("retry count"))?;
        self.cumulative_delay_millis = cumulative_delay_millis;
        Ok(ConditionalBackoffDecision {
            retry_index,
            delay_millis,
            cumulative_delay_millis,
            used_server_minimum: server_minimum > exponential,
        })
    }
}

#[cfg(test)]
mod conditional_backoff_tests {
    use super::*;

    fn policy() -> ConditionalBackoffPolicy {
        ConditionalBackoffPolicy::new(100, 1_000, 5_000).expect("policy")
    }

    #[test]
    fn exponential_delays_are_capped_deterministically() {
        let mut budget = ConditionalBackoffBudget::new(policy());
        let delays: Vec<_> = (0..6)
            .map(|_| {
                budget
                    .plan_next_delay(None, None)
                    .expect("bounded delay")
                    .delay_millis
            })
            .collect();
        assert_eq!(delays, vec![100, 200, 400, 800, 1_000, 1_000]);
        assert_eq!(budget.retries_planned(), 6);
        assert_eq!(budget.cumulative_delay_millis(), 3_500);
    }

    #[test]
    fn bounded_server_minimum_is_honoured_without_truncation() {
        let mut budget = ConditionalBackoffBudget::new(policy());
        let decision = budget
            .plan_next_delay(Some(750), None)
            .expect("server minimum");
        assert_eq!(decision.delay_millis, 750);
        assert!(decision.used_server_minimum);
        assert_eq!(
            budget.plan_next_delay(Some(1_001), None),
            Err(ConditionalSourceError::Limit("server retry delay"))
        );
        assert_eq!(budget.retries_planned(), 1);
    }

    #[test]
    fn cumulative_budget_is_charged_only_after_an_accepted_plan() {
        let constrained = ConditionalBackoffPolicy::new(100, 1_000, 250).expect("policy");
        let mut budget = ConditionalBackoffBudget::new(constrained);
        assert_eq!(
            budget
                .plan_next_delay(None, None)
                .expect("first")
                .delay_millis,
            100
        );
        assert_eq!(
            budget.plan_next_delay(None, None),
            Err(ConditionalSourceError::Limit("retry delay"))
        );
        assert_eq!(budget.retries_planned(), 1);
        assert_eq!(budget.cumulative_delay_millis(), 100);
    }

    #[test]
    fn deadline_rejects_a_wait_that_reaches_the_deadline() {
        let mut budget = ConditionalBackoffBudget::new(policy());
        assert_eq!(
            budget.plan_next_delay(None, Some(100)),
            Err(ConditionalSourceError::DeadlineExceeded)
        );
        assert_eq!(budget.retries_planned(), 0);
        assert_eq!(
            budget
                .plan_next_delay(None, Some(101))
                .expect("before deadline")
                .delay_millis,
            100
        );
    }

    #[test]
    fn invalid_policy_is_rejected() {
        assert_eq!(
            ConditionalBackoffPolicy::new(0, 1, 1),
            Err(ConditionalSourceError::Limit("retry delay configuration"))
        );
        assert_eq!(
            ConditionalBackoffPolicy::new(2, 1, 2),
            Err(ConditionalSourceError::Limit("retry delay configuration"))
        );
        assert_eq!(
            ConditionalBackoffPolicy::new(2, 2, 1),
            Err(ConditionalSourceError::Limit("retry delay configuration"))
        );
    }
}
