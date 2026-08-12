# Experiment 0089: Conditional authentication refresh execution

## Question

Can one explicitly authorized authentication refresh be executed without granting hidden provider policy, consuming backoff state, or allowing an unbounded 401 loop?

## Policy

The caller supplies an adapter-neutral conditional HTTP exchange, an application-owned authentication refresher, operation cancellation/deadline control, and an explicit authentication policy.

The first response is classified by the existing fail-closed HTTP classifier. Only `RefreshAuthentication` may invoke the refresher, and only when `OneRefreshPermitted` was supplied. After one successful refresh, the exact same request is replayed once and the replay is classified with terminal authentication policy. A second 401 is therefore terminal and cannot invoke the refresher again.

Cancellation and the monotonic operation deadline are checked before and after each exchange and refresh call. Transport and refresh failures return directly. This layer neither plans nor executes backoff and does not retry transient transport errors.

## Evidence

Rust tests cover:

- one authorized refresh followed by one successful replay;
- exact exchange and refresh attempt counts;
- terminal classification of a second 401;
- terminal policy without any refresher call;
- direct propagation of refresh failures;
- direct propagation of transient transport failures without hidden retry;
- cancellation after refresh preventing the replay.

The `conditional_authentication_refresh` fuzz target varies initial and replay statuses, authorization, refresh failure, transport failure, and cancellation. It checks that exchange calls never exceed two, refresh calls never exceed one, terminal policy never refreshes, and every replay is classified with terminal authentication policy.

## Boundary

This is not a maintained HTTP or cloud adapter. It does not acquire credentials, define credential lifetime, infer provider-specific 401 semantics, follow redirects, issue request bodies, select backoff, wait, or integrate a native asynchronous runtime. It proves only the bounded adapter-neutral refresh/replay state machine. Production credential storage, redaction, refresh synchronization, provider qualification, and native asynchronous cancellation remain separate requirements.
