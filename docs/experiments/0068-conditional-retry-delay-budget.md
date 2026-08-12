# Experiment 0068: conditional retry delay budgeting

## Question

Can retry delay decisions remain bounded, deadline-aware, deterministic, and separate from provider-specific status classification?

## Planner

The pure delay budget:

- requires non-zero base delay, a per-delay cap no smaller than the base, and a cumulative cap;
- computes capped exponential delays by retry index;
- accepts a server-provided minimum only when it fits the configured per-delay cap;
- rejects rather than truncates a server minimum that cannot be honoured;
- rejects a wait that reaches or crosses the remaining deadline;
- charges the cumulative budget only after a plan is accepted;
- uses checked retry and delay accounting.

## Evidence

Pinned cases cover capped exponential progression, server-minimum selection, server-minimum rejection, cumulative exhaustion without partial charging, deadline equality, and invalid policy construction.

## Boundary

The planner does not sleep, add jitter, classify provider responses, refresh credentials, follow redirects, or interrupt a blocking synchronous call. A concrete adapter must check shared cancellation and monotonic deadlines immediately before and after any real wait.
