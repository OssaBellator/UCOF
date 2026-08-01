# Experiment 0088: Conditional retry wait execution

## Question

Can a conditional-source retry delay be executed with reproducible jitter, bounded cancellation latency, monotonic deadline checks, and no hidden transport-specific timing policy?

## Policy

A previously accepted backoff decision is combined with one caller-supplied jitter sample. The sample is rejected rather than truncated when it exceeds the configured jitter bound. The total planned wait must finish strictly before the caller's remaining deadline.

Execution divides the accepted delay into cooperative chunks no larger than the configured polling interval. Cancellation and the operation's monotonic deadline are checked before and after every sleeper call. A sleeper failure is terminal for that wait and returns no success report.

The library provides a synchronous thread sleeper, while tests, native asynchronous runtimes, and maintained provider adapters may inject another sleeper. Random jitter generation is deliberately outside this layer so adapters can make the seed, distribution, and reproducibility policy explicit.

## Evidence

Rust tests cover:

- exact deterministic jitter addition;
- exact chunk boundaries and accumulated delay;
- rejection of excessive jitter;
- rejection of a wait that reaches the deadline;
- cancellation after an arbitrary completed chunk;
- expired controls before any sleeper call;
- terminal sleeper failure without a success report;
- availability of a real synchronous thread wait.

The `conditional_wait_executor` fuzz target varies base delays, jitter bounds and samples, polling intervals, remaining deadlines, sleeper failure points, and cancellation points. It checks exact successful chunk sums, bounded positive chunks, terminal failures, and bounded cancellation observation.

## Boundary

This is a synchronous cooperative wait executor, not a maintained HTTP or cloud adapter. It does not select a jitter distribution, seed entropy, retry status, authentication refresh, transport request, or provider-specific timeout. A blocking sleeper cannot observe cancellation inside one chunk, so cancellation latency is bounded by the polling interval rather than instantaneous. Native asynchronous cancellation and production timer/runtime qualification remain separate requirements.
