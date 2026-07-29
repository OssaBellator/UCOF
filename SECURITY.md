# Security Policy

UCOF is intended to parse untrusted files. Security reports are therefore treated as first-class engineering input even before a stable implementation exists.

## Project status

UCOF is experimental. There is currently no stable wire format or supported production release. Prototype files and APIs may change incompatibly and must not be treated as a security boundary.

## Supported versions

Until the first tagged software release, only the current default branch is maintained. After releases begin, this document will contain an explicit support table.

| Version | Supported |
|---|---|
| Default branch | Yes |
| Unreleased historical commits | No |
| Experimental file epochs superseded by a newer epoch | No |

## Reporting a vulnerability

Do not disclose an unpatched vulnerability in a public issue, pull request, discussion, test fixture, or commit message.

Preferred reporting path:

1. Use GitHub private vulnerability reporting for this repository when the **Report a vulnerability** option is available under the Security tab.
2. If private vulnerability reporting is unavailable, contact the repository owner or a listed maintainer through an available private GitHub channel.
3. If no private route is available, open a minimal public issue requesting private contact. Do not include exploit details, malicious samples, affected offsets, identities, or reproduction steps.

Include when possible:

- affected commit, release, file epoch, profile, or component;
- impact and realistic attack scenario;
- minimal reproduction steps;
- a non-public sample or generator, if safe to share;
- expected versus observed behavior;
- proposed mitigation, if known;
- whether the issue is already public or under coordinated disclosure elsewhere.

Do not send real confidential data or malware when a synthetic reproducer can demonstrate the issue.

## Response process

Maintainers will aim to:

1. acknowledge receipt;
2. establish a private coordination channel;
3. reproduce and classify the issue;
4. identify affected versions and design assumptions;
5. prepare a fix, test, advisory, or specification clarification;
6. coordinate disclosure with the reporter;
7. update the threat model and malformed corpus when appropriate.

Exact response deadlines are not promised while the project has a single-maintainer governance model. Reporters should state any external disclosure deadline at first contact.

## Security-sensitive areas

Reports are especially valuable for:

- integer overflow, underflow, truncation, or offset wraparound;
- overlapping, aliased, or contradictory byte ranges;
- unbounded allocation, recursion, nesting, object traversal, or diagnostics;
- decompression and transform expansion;
- parser differentials between strict and permissive readers;
- acceptance of stale, attacker-selected, or partially verified roots;
- hash algorithm confusion, substitution, or ambiguous canonicalization;
- signature wrapping or unclear signed scope;
- encryption nonce misuse, key confusion, or metadata leakage;
- malicious schema, dictionary, index, or external-reference behavior;
- denial of service through adversarial but syntactically valid files;
- unsafe temporary-file, extraction-path, or symlink handling in tools;
- unexpected code execution or network access.

## Disclosure and credit

The project prefers coordinated disclosure after a fix or clear mitigation is available. Security advisories should describe affected versions, severity rationale, mitigations, and any format-level implications.

Reporters will be credited when requested, unless legal, privacy, or safety concerns prevent it.

## Out of scope

The following are generally not vulnerabilities by themselves:

- incompatibility between explicitly experimental file epochs;
- failure to interpret an unsupported optional profile;
- high resource use that remains within documented caller-configured limits;
- authenticity claims made without a trusted key or trust policy;
- confidentiality assumptions contradicted by documented public metadata.

These may still justify usability, documentation, or design improvements.
