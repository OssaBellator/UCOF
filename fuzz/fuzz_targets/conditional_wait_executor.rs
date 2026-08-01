#![no_main]

use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    execute_conditional_wait, plan_conditional_wait, ConditionalBackoffDecision,
    ConditionalSleeper, ConditionalSourceError, ConditionalWaitPolicy,
    ImmutableCancellationHandle, ImmutableOperationControl,
};

#[derive(Default)]
struct FuzzSleeper {
    calls: Vec<u64>,
    fail_at: Option<usize>,
    cancel_after: Option<usize>,
    cancellation: Option<ImmutableCancellationHandle>,
}

impl ConditionalSleeper for FuzzSleeper {
    fn sleep(&mut self, duration: Duration) -> Result<(), ConditionalSourceError> {
        let index = self.calls.len();
        if self.fail_at == Some(index) {
            return Err(ConditionalSourceError::Client("fuzz wait failure"));
        }
        let millis = u64::try_from(duration.as_millis())
            .map_err(|_| ConditionalSourceError::Limit("wait duration"))?;
        self.calls.push(millis);
        if self.cancel_after == Some(self.calls.len()) {
            self.cancellation.as_ref().expect("cancellation").cancel();
        }
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    let byte = |index: usize| data.get(index).copied().unwrap_or(0);
    let base = 1 + u64::from(byte(0));
    let jitter_limit = u64::from(byte(1));
    let jitter = u64::from(byte(2));
    let poll = 1 + u64::from(byte(3) % 64);
    let deadline = base
        .checked_add(jitter)
        .and_then(|value| value.checked_add(1 + u64::from(byte(4))))
        .expect("small deadline");
    let decision = ConditionalBackoffDecision {
        retry_index: u32::from(byte(5)),
        delay_millis: base,
        cumulative_delay_millis: base,
        used_server_minimum: byte(6) & 1 != 0,
    };
    let planned = plan_conditional_wait(
        decision,
        jitter,
        ConditionalWaitPolicy::new(jitter_limit, poll).expect("valid poll"),
        Some(deadline),
    );
    if jitter > jitter_limit || base + jitter >= deadline {
        assert!(planned.is_err());
        return;
    }
    let plan = planned.expect("bounded wait plan");
    assert_eq!(plan.total_delay_millis, base + jitter);
    assert_eq!(plan.jitter_millis, jitter);
    assert!(plan.poll_interval_millis > 0);

    let (control, handle) = ImmutableOperationControl::new(None);
    let possible_calls = usize::try_from(
        plan.total_delay_millis
            .div_ceil(plan.poll_interval_millis),
    )
    .expect("small calls");
    let mode = byte(7) % 3;
    let trigger = if possible_calls == 0 {
        0
    } else {
        usize::from(byte(8)) % possible_calls
    };
    let mut sleeper = FuzzSleeper {
        fail_at: (mode == 1).then_some(trigger),
        cancel_after: (mode == 2).then_some(trigger + 1),
        cancellation: (mode == 2).then_some(handle),
        ..FuzzSleeper::default()
    };
    let result = execute_conditional_wait(&control, &mut sleeper, plan);
    match mode {
        0 => {
            let report = result.expect("complete wait");
            assert_eq!(report.completed_delay_millis, plan.total_delay_millis);
            assert_eq!(report.requested_delay_millis, plan.total_delay_millis);
            assert_eq!(
                sleeper.calls.iter().copied().sum::<u64>(),
                plan.total_delay_millis
            );
            assert!(
                sleeper
                    .calls
                    .iter()
                    .all(|chunk| *chunk > 0 && *chunk <= plan.poll_interval_millis)
            );
        }
        1 => {
            assert_eq!(
                result,
                Err(ConditionalSourceError::Client("fuzz wait failure"))
            );
            assert_eq!(sleeper.calls.len(), trigger);
        }
        _ => {
            assert_eq!(result, Err(ConditionalSourceError::Cancelled));
            assert_eq!(sleeper.calls.len(), trigger + 1);
        }
    }
});
