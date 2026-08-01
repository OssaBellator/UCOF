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

## Measured rows

The green CI run on Rust 1.97.1 produced:

| Objects | File bytes | Root level | Strict reads | Strict bytes | Lookup reads | Lookup bytes | Rewrite reads | Rewrite bytes | Largest allocation |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 16,784 | 0 | 39 | 33,568 | 38 | 33,440 | 76 | 67,008 | 16,384 |
| 185 | 37,392 | 0 | 427 | 74,784 | 58 | 54,048 | 485 | 128,944 | 16,384 |
| 400 | 110,624 | 1 | 976 | 221,248 | 145 | 143,664 | 1,153 | 397,792 | 16,384 |
| 1,000 | 226,976 | 1 | 2,338 | 453,952 | 259 | 260,016 | 2,677 | 796,000 | 16,384 |

For the 1,000-object case, one authenticated lookup used about 57% of the bytes read by complete validation while preserving the same 16 KiB maximum temporary allocation. Selected rewrite performs complete validation plus inventory and selected-record rereads, so its counters are intentionally higher than strict validation.

## Execution

CI runs:

```text
cargo run --locked -p ucof-experiments --bin ucof-source-io-profile
```

The output is intended for review and regression comparison. It is not a latency benchmark and does not pin machine-dependent elapsed time.

## Boundary

The profile uses an in-memory slice source and deterministic synthetic payloads. It does not measure network round trips, provider billing, cache behavior, kernel readahead, concurrency, or end-to-end wall-clock performance. Concrete HTTP and cloud measurements remain required.
