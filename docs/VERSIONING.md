# UCOF Versioning and Compatibility

## 1. Separate version domains

UCOF uses separate identifiers for:

1. **software releases** — versions of libraries, tools, and implementations;
2. **specification releases** — versions of the normative core or a profile;
3. **experimental wire epochs** — intentionally disposable pre-stable serialized layouts;
4. **registry revisions** — additions and status changes in identifier registries.

These identifiers must not be treated as interchangeable.

## 2. Software versions

Reference implementation crates and command-line tools follow Semantic Versioning once they are released.

- `MAJOR` changes may break public APIs or supported behavior.
- `MINOR` changes add backward-compatible APIs or features.
- `PATCH` changes fix defects without intentionally breaking supported APIs.

Before software version `1.0.0`, minor releases may contain breaking API changes. Release notes must identify them.

A software version does not imply support for a specification version unless the release documentation says so explicitly.

## 3. Core specification versions

The normative core uses `MAJOR.MINOR` identifiers, such as `0.1`, `0.2`, and eventually `1.0`.

### 3.1 Major version

The major version changes when a conforming file or required behavior can no longer be interpreted according to the preceding major version without an explicit compatibility mechanism.

A new major version requires a migration and coexistence plan.

### 3.2 Minor version

After `1.0`, the minor version may add compatible capabilities, object types, constraints, or clarifications that preserve the core framing contract and established interpretation of existing valid files.

A minor version must not silently reinterpret bytes that were valid under an earlier minor version.

### 3.3 Editorial revisions and errata

Spelling corrections, formatting, non-normative examples, and clarifications that do not change conformance may be published as dated revisions or errata without changing `MAJOR.MINOR`.

An erratum that changes previously required behavior is normative and must follow the proposal process. Security emergencies may use the expedited process in the governance document.

## 4. Profile versions

Each profile versions independently from the core and declares:

- its profile identifier;
- its profile version;
- the minimum and maximum core versions it targets, when applicable;
- required capabilities and registry entries;
- compatibility rules for readers and writers.

A profile version must not imply support for all later core versions.

## 5. Experimental wire epochs

Before a stable format release, incompatible byte layouts are identified by a monotonic experimental epoch:

```text
UCOF-EXP-0001
UCOF-EXP-0002
...
```

The exact on-disk encoding of the epoch will be decided during Phase 1, but every experimental file writer must expose the epoch unambiguously in its bootstrap data and diagnostic output.

Rules:

- an incompatible serialized-layout change increments the epoch;
- an epoch is never reused;
- experimental epochs do not become stable specification versions;
- no backward compatibility is promised across epochs;
- writers must not emit an obsolete epoch after support is removed;
- readers may support multiple epochs, but must report which one was detected;
- an unknown epoch must fail as unsupported rather than be guessed;
- fixtures must state the epoch in filenames or adjacent metadata;
- experimental files are unsuitable for durable archival storage.

### 5.1 Invalidating an experimental epoch

An epoch becomes **superseded** when a later epoch is accepted for active development. It becomes **retired** when the reference implementation no longer reads or writes it.

Retirement requires:

- a repository notice or release note;
- migration guidance when a practical converter exists;
- preserved historical fixtures where useful for regression testing;
- removal from default writer output;
- explicit failure or opt-in legacy handling in readers.

The project may immediately retire an epoch with a severe security or ambiguity defect.

## 6. Capability negotiation

Version numbers are not a substitute for capability declarations.

Files and profiles should identify required, optional, and advisory capabilities so that a reader can determine whether safe partial handling is possible. A reader must reject an unknown required capability even when it recognizes the surrounding specification version.

## 7. Registry revisions

Registries use append-only allocation history and a revision identifier or publication date. Adding a new identifier does not necessarily change the core specification version.

Existing assigned identifiers must not be silently repurposed. Status changes such as deprecated or withdrawn require a recorded reason and replacement guidance where applicable.

See [REGISTRY_POLICY.md](REGISTRY_POLICY.md).

## 8. Compatibility claims

Implementations must state compatibility narrowly. Preferred wording includes:

- “reads UCOF Core 0.3 experimental subset X”;
- “writes experimental epoch UCOF-EXP-0002”;
- “conforms to UCOF Core 1.0 reader requirements”;
- “supports Archive Profile 1.1 required features.”

Avoid ambiguous claims such as “supports UCOF” without identifying reader/writer role, core version, profile version, and unsupported required capabilities.

## 9. Pre-1.0 policy

Until UCOF Core 1.0:

- all serialized layouts are unstable;
- stable archival compatibility is not promised;
- permanent registry identifiers are allocated only through accepted proposals;
- each prototype must identify its experimental epoch;
- incompatible changes must be recorded in release notes or proposal history;
- migration tooling is best effort rather than a compatibility guarantee.

## 10. 1.0 stabilization bar

Core 1.0 must not be published until:

- the normative byte layout is complete and unambiguous;
- canonicalization rules have deterministic vectors;
- security and resource-limit requirements are defined;
- recovery and truncation behavior is testable;
- at least one independent implementation demonstrates interoperability;
- accepted registries and extension rules exist;
- compatibility and errata processes are operational;
- the Phase 0 use cases and threat model have been revisited against the implemented design.
