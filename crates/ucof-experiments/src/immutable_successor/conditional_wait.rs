use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionalWaitPolicy {
    pub max_jitter_millis: u64,
    pub poll_interval_millis: u64,
}

impl ConditionalWaitPolicy {
    pub fn new(
        max_jitter_millis: u64,
        poll_interval_millis: u64,
    ) -> Result<Self, ConditionalSourceError> {
        if poll_interval_millis == 0 {
            return Err(ConditionalSourceError::Limit("wait poll interval"));
        }
        Ok(Self {
            max_jitter_millis,
            poll_interval_millis,
        })
    }
}

impl Default for ConditionalWaitPolicy {
    fn default() -> Self {
        Self {
            max_jitter_millis: 250,
            poll_interval_millis: 50,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionalWaitPlan {
    pub retry_index: u32,
    pub base_delay_millis: u64,
    pub jitter_millis: u64,
    pub total_delay_millis: u64,
    pub poll_interval_millis: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConditionalWaitReport {
    pub retry_index: u32,
    pub requested_delay_millis: u64,
    pub completed_delay_millis: u64,
    pub wait_calls: u64,
}

pub trait ConditionalSleeper {
    fn sleep(&mut self, duration: Duration) -> Result<(), ConditionalSourceError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadSleeper;

impl ConditionalSleeper for ThreadSleeper {
    fn sleep(&mut self, duration: Duration) -> Result<(), ConditionalSourceError> {
        std::thread::sleep(duration);
        Ok(())
    }
}

/// Adds one caller-supplied deterministic jitter sample to a previously accepted backoff decision.
///
/// The sample is never generated internally, so tests and maintained adapters can reproduce it. A
/// sample above the configured bound is rejected rather than truncated. The combined wait must end
/// strictly before the supplied remaining deadline.
pub fn plan_conditional_wait(
    decision: ConditionalBackoffDecision,
    jitter_millis: u64,
    policy: ConditionalWaitPolicy,
    remaining_deadline_millis: Option<u64>,
) -> Result<ConditionalWaitPlan, ConditionalSourceError> {
    if policy.poll_interval_millis == 0 {
        return Err(ConditionalSourceError::Limit("wait poll interval"));
    }
    if jitter_millis > policy.max_jitter_millis {
        return Err(ConditionalSourceError::Limit("retry jitter"));
    }
    let total_delay_millis = decision
        .delay_millis
        .checked_add(jitter_millis)
        .ok_or(ConditionalSourceError::Limit("retry wait"))?;
    if remaining_deadline_millis.is_some_and(|remaining| total_delay_millis >= remaining) {
        return Err(ConditionalSourceError::DeadlineExceeded);
    }
    Ok(ConditionalWaitPlan {
        retry_index: decision.retry_index,
        base_delay_millis: decision.delay_millis,
        jitter_millis,
        total_delay_millis,
        poll_interval_millis: policy.poll_interval_millis,
    })
}

/// Executes a planned retry wait in bounded cooperative chunks.
///
/// Cancellation and the monotonic operation deadline are checked immediately before and after every
/// sleeper call. A standard `ThreadSleeper` performs real synchronous waits, while tests and native
/// asynchronous adapters can provide another sleeper. Cancellation latency for `ThreadSleeper` is
/// bounded by the poll interval rather than the complete delay. A sleeper error or failed control
/// check returns no success report.
pub fn execute_conditional_wait<S: ConditionalSleeper>(
    control: &ImmutableOperationControl,
    sleeper: &mut S,
    plan: ConditionalWaitPlan,
) -> Result<ConditionalWaitReport, ConditionalSourceError> {
    control.check()?;
    let mut report = ConditionalWaitReport {
        retry_index: plan.retry_index,
        requested_delay_millis: plan.total_delay_millis,
        ..ConditionalWaitReport::default()
    };
    while report.completed_delay_millis < plan.total_delay_millis {
        control.check()?;
        let remaining = plan
            .total_delay_millis
            .checked_sub(report.completed_delay_millis)
            .ok_or(ConditionalSourceError::Limit("retry wait"))?;
        let chunk = remaining.min(plan.poll_interval_millis);
        sleeper.sleep(Duration::from_millis(chunk))?;
        report.wait_calls = report
            .wait_calls
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("wait calls"))?;
        report.completed_delay_millis = report
            .completed_delay_millis
            .checked_add(chunk)
            .ok_or(ConditionalSourceError::Limit("retry wait"))?;
        control.check()?;
    }
    Ok(report)
}

#[cfg(test)]
mod conditional_wait_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct RecordingSleeper {
        calls: Vec<u64>,
        fail_at: Option<usize>,
        cancel_after: Option<usize>,
        cancellation: Option<ImmutableCancellationHandle>,
    }

    impl ConditionalSleeper for RecordingSleeper {
        fn sleep(&mut self, duration: Duration) -> Result<(), ConditionalSourceError> {
            let index = self.calls.len();
            if self.fail_at == Some(index) {
                return Err(ConditionalSourceError::Client("injected wait failure"));
            }
            self.calls.push(
                u64::try_from(duration.as_millis())
                    .map_err(|_| ConditionalSourceError::Limit("wait duration"))?,
            );
            if self.cancel_after == Some(self.calls.len()) {
                self.cancellation.as_ref().expect("cancellation").cancel();
            }
            Ok(())
        }
    }

    fn decision(delay_millis: u64) -> ConditionalBackoffDecision {
        ConditionalBackoffDecision {
            retry_index: 2,
            delay_millis,
            cumulative_delay_millis: delay_millis,
            used_server_minimum: false,
        }
    }

    #[test]
    fn deterministic_jitter_and_chunking_are_exact() {
        let plan = plan_conditional_wait(
            decision(100),
            25,
            ConditionalWaitPolicy::new(50, 40).expect("policy"),
            Some(200),
        )
        .expect("plan");
        assert_eq!(plan.total_delay_millis, 125);
        let mut sleeper = RecordingSleeper::default();
        let report = execute_conditional_wait(
            &ImmutableOperationControl::unlimited(),
            &mut sleeper,
            plan,
        )
        .expect("wait");
        assert_eq!(sleeper.calls, vec![40, 40, 40, 5]);
        assert_eq!(report.retry_index, 2);
        assert_eq!(report.requested_delay_millis, 125);
        assert_eq!(report.completed_delay_millis, 125);
        assert_eq!(report.wait_calls, 4);
    }

    #[test]
    fn jitter_and_deadline_fail_before_waiting() {
        assert_eq!(
            plan_conditional_wait(
                decision(100),
                51,
                ConditionalWaitPolicy::new(50, 10).expect("policy"),
                None,
            ),
            Err(ConditionalSourceError::Limit("retry jitter"))
        );
        assert_eq!(
            plan_conditional_wait(
                decision(100),
                20,
                ConditionalWaitPolicy::new(50, 10).expect("policy"),
                Some(120),
            ),
            Err(ConditionalSourceError::DeadlineExceeded)
        );
    }

    #[test]
    fn cancellation_is_detected_after_a_bounded_chunk() {
        let (control, handle) = ImmutableOperationControl::new(None);
        let mut sleeper = RecordingSleeper {
            cancel_after: Some(2),
            cancellation: Some(handle),
            ..RecordingSleeper::default()
        };
        let error = execute_conditional_wait(
            &control,
            &mut sleeper,
            ConditionalWaitPlan {
                retry_index: 0,
                base_delay_millis: 100,
                jitter_millis: 0,
                total_delay_millis: 100,
                poll_interval_millis: 30,
            },
        )
        .expect_err("cancelled");
        assert_eq!(error, ConditionalSourceError::Cancelled);
        assert_eq!(sleeper.calls, vec![30, 30]);
    }

    #[test]
    fn expired_control_and_sleeper_failure_return_no_report() {
        let (expired, _) = ImmutableOperationControl::new(Some(Instant::now()));
        let mut sleeper = RecordingSleeper::default();
        assert_eq!(
            execute_conditional_wait(
                &expired,
                &mut sleeper,
                ConditionalWaitPlan {
                    retry_index: 0,
                    base_delay_millis: 1,
                    jitter_millis: 0,
                    total_delay_millis: 1,
                    poll_interval_millis: 1,
                },
            ),
            Err(ConditionalSourceError::DeadlineExceeded)
        );
        assert!(sleeper.calls.is_empty());

        let mut failing = RecordingSleeper {
            fail_at: Some(1),
            ..RecordingSleeper::default()
        };
        assert_eq!(
            execute_conditional_wait(
                &ImmutableOperationControl::unlimited(),
                &mut failing,
                ConditionalWaitPlan {
                    retry_index: 1,
                    base_delay_millis: 20,
                    jitter_millis: 0,
                    total_delay_millis: 20,
                    poll_interval_millis: 10,
                },
            ),
            Err(ConditionalSourceError::Client("injected wait failure"))
        );
        assert_eq!(failing.calls, vec![10]);
    }

    #[test]
    fn real_thread_wait_is_available_for_synchronous_adapters() {
        let plan = ConditionalWaitPlan {
            retry_index: 0,
            base_delay_millis: 1,
            jitter_millis: 0,
            total_delay_millis: 1,
            poll_interval_millis: 1,
        };
        let start = Instant::now();
        let report = execute_conditional_wait(
            &ImmutableOperationControl::new(Some(Instant::now() + Duration::from_secs(1))).0,
            &mut ThreadSleeper,
            plan,
        )
        .expect("real wait");
        assert_eq!(report.completed_delay_millis, 1);
        assert!(start.elapsed() >= Duration::from_millis(1));
    }
}
