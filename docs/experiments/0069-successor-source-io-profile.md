# Experiment 0069: reproducible successor source I/O profile

## Question

Can range-source work be reported reproducibly across tree sizes without substituting wall-clock timing for authenticated read, hash, and allocation accounting?

## Profile

The executable profile constructs deterministic files with `1`, `185`, `400`, and `1,000` objects and reports CSV rows for:

- file bytes and root level;
- complete strict-validation read operations, bytes read, and bytes hashed;
- one authenticated lookup's read operations, bytes read, and bytes hashed;
- a one- or two-object selected source rewrite's read operations, bytes read, and bytes hashed;
- the largest temporary allocation observed by each assurance path.

Every source request is limited to `1,024` bytes. The profile fails if path lookup reads more bytes than complete validation, selected rewrite does not include strict-validation work, the selected output count is wrong, or a temporary allocation reaches the full source-file size.

## Execution

CI runs:

```text
cargo run --locked -p ucof-experiments --bin ucof-source-io-profile
```

The output is intended for review and regression comparison. It is not a latency benchmark and does not pin machine-dependent elapsed time.

## Boundary

The profile uses an in-memory slice source and deterministic synthetic payloads. It does not measure network round trips, provider billing, cache behavior, kernel readahead, concurrency, or end-to-end wall-clock performance. Concrete HTTP and cloud measurements remain required.
