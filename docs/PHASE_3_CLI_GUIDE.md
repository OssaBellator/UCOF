# Phase 3 Experimental CLI Guide

The `ucof-exp0002` binary exercises the disposable Candidate 1 implementation in `ucof-experiments`. It is non-normative, unpublished, and not compatible with a future stable UCOF version merely because a command succeeds.

Run commands with:

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- <command> ...
```

## Assurance matrix

| Command | What it establishes | What it does not establish |
|---|---|---|
| `verify` | Full exact-end validation of the active commit, snapshot, every reachable page, every referenced object, roots, parent link, and commit digest | Authenticity, trusted freshness, rollback resistance, confidentiality, or recovery of damaged tails |
| `roots` | Root identifiers from a fully validated active exact-end snapshot | Historical-root enumeration or recovery selection |
| `history` | The exact-end active commit and every linked ancestor validated independently as strict prefixes, including sequence and parent-digest relationships | Discovery of unlinked candidates, fork resolution, or trusted freshness |
| `lookup` | Active commit and snapshot integrity, one authenticated directory path, and the selected object; may prove authenticated absence | Full validation of unrelated historical objects |
| `recover` | Strictly valid file prefixes discovered by an explicitly requested bounded suffix scan | Selection of a preferred candidate, trusted freshness, or normal-file validity |
| `repair-all` | A strictly verified source rewritten as a new validated genesis file containing every active object | Preservation of file-instance commit identity or byte-scoped signatures |
| `rewrite-selected` | Caller-selected authenticated objects and roots rewritten as a new validated genesis file | Automatic semantic dependency discovery or a claim that the result is complete semantic compaction |

`verify` never invokes `recover`. A command failure is never converted into a lower-assurance success.

## Verify

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- \
  verify archive.ucof
```

The report includes sequence, exact footer offset, object and page counts, roots, snapshot and commit digests, and source-read statistics.

## Active roots

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- \
  roots archive.ucof
```

This command first performs the same full strict validation as `verify`. It does not scan historical candidates.

## Verified linked history

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- \
  history archive.ucof
```

The active file and every previous-footer ancestor are each validated as exact-end prefixes. The command cross-checks that each child points to the validated parent footer, authenticates the parent snapshot digest, and increments sequence by exactly one. Work is bounded by chain depth and cumulative source reads.

Entries are printed from the active commit toward genesis with roots, previous-footer offset, parent snapshot digest, snapshot digest, and commit digest. The command does not search for unlinked footer candidates and does not silently resolve forks.

## Authenticated lookup

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- \
  lookup archive.ucof 42
```

A successful match reports the physical record and payload ranges and the work performed. A missing match is an authenticated absence result for the active snapshot.

## Explicit recovery

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- \
  recover damaged.ucof
```

Recovery reports every candidate that passed full exact-end validation as a prefix. Each result includes roots and parent, snapshot, and commit identities. The command independently bounds scan bytes, scan read operations, footer-magic matches, candidate validations, returned results, and cumulative candidate reads. Reads spent rejecting malformed candidates are charged to the cumulative budget.

The CLI does not choose the newest acceptable root for an application. Sequence numbers and stored links do not provide external freshness.

## Repair all active objects

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- \
  repair-all input.ucof repaired.ucof \
  00112233445566778899aabbccddeeff \
  102132435465768798a9bacbdcedfe0f
```

The two hexadecimal arguments are exactly 16 bytes each and become the new file identifier and creation nonce.

The source is fully read into memory by the current rewrite implementation. The command strictly validates it, builds and validates a new genesis file, and only then creates the output. The output path must not already exist. After writing, the command requests `sync_all` from the host filesystem.

## Caller-selected rewrite

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- \
  rewrite-selected input.ucof compacted.ucof \
  00112233445566778899aabbccddeeff \
  102132435465768798a9bacbdcedfe0f \
  1,2,8,13 \
  1,8
```

The retained-object and root lists are comma-separated non-zero decimal identifiers. The CLI sorts and deduplicates the lists before invoking the strict rewrite API. Every root must be retained and every retained identifier must exist.

This operation is deliberately named `rewrite-selected`, not `compact`. Without schema or profile dependency semantics, the generic container cannot prove that omitted objects are semantically unnecessary.

## Current implementation boundaries

- Validation, lookup, history, and recovery use a synchronous seekable source and bounded read requests.
- The CLI operates on local file handles and assumes one stable local view for a command.
- The library provides `Exp0002StableSource` for transports with a strong caller-supplied version token; token changes fail before or after every range read. Stable view is not trusted freshness.
- Rewrite commands currently materialize the source and output in memory.
- Default resource limits are implementation-local and not normative minima.
- Candidate 1 provides SHA-256 integrity relative to stored values, not signatures or trust.
- Output identities are reported at both snapshot and commit scope; byte-scoped signatures are always reported as not preserved.
