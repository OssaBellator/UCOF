# Architecture Decision Records

Architecture Decision Records (ADRs) document consequential choices in the reference implementation and project infrastructure.

ADRs are appropriate for:

- crate or module boundaries;
- internal API design;
- error representation;
- build, test, fuzzing, and release tooling;
- implementation dependencies;
- repository and CI architecture;
- benchmark methodology;
- internal storage choices that do not affect interoperable bytes or required behavior.

ADRs are not sufficient for:

- wire-format bytes;
- canonicalization;
- required reader or writer behavior;
- profiles or capabilities;
- permanent identifiers;
- compatibility or security-critical format semantics.

Those require a [Format Change Proposal](../PROPOSAL_PROCESS.md).

## Naming

Use four-digit numbers and short kebab-case titles:

```text
0001-rust-workspace-layout.md
0002-structured-error-model.md
```

Numbers are never reused.

## Status

An ADR is one of:

- Proposed
- Accepted
- Rejected
- Superseded
- Deprecated

A later ADR may supersede an earlier one. Preserve the earlier record and add reciprocal links.

## Process

1. Copy [TEMPLATE.md](TEMPLATE.md).
2. Select the next unused number.
3. Open a pull request with the ADR in Proposed status.
4. Record context, constraints, options, and consequences.
5. Obtain maintainer approval.
6. Update status and merge.

Small reversible choices do not require an ADR. Prefer an ADR when future contributors would otherwise need to rediscover why an option was selected.
