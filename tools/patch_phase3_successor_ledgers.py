#!/usr/bin/env python3
from pathlib import Path

status_path = Path("docs/PHASE_3_STATUS.md")
status = status_path.read_text(encoding="utf-8")
replacements = {
    "They currently cover complete objects, immutable pages, recursive tree updates, bounded source access, metadata catalogs, recovery, spill-backed writing, and one independently parsed exact-end vector. No successor compatibility promise exists.":
    "They currently cover complete objects, immutable pages, recursive tree updates, a reusable Rust slice writer/strict-validator/history/recovery/rewrite core, bounded source models, metadata catalogs, spill-backed writing, and one independently parsed exact-end vector. No successor compatibility promise exists.",
    "- Current implementation limits are policy ceilings, not normative conformance minima, under ADR-0015.\n":
    "- Current implementation limits are policy ceilings, not normative conformance minima, under ADR-0015.\n- Exact-end validity, linked-history verification, and report-only suffix recovery remain separate Rust assurance scopes under ADR-0016.\n",
    "- twenty-one cargo-fuzz target builds and bounded smoke campaigns.":
    "- twenty-five cargo-fuzz target builds and bounded smoke campaigns.",
    "- successor implementations remain Python experiments plus one independent Rust vector parser;":
    "- successor implementations remain Python models plus a reusable Rust slice writer/validator/history/recovery/rewrite experiment;",
    "- no production successor writer, source adapter, recovery, history, or repair library exists;":
    "- no production random-access/conditional source adapter, streaming or spill-integrated writer, or hardened repair/publication library exists;",
    "5. Move successor parsing and validation into a reusable Rust experiment module, then add fuzz targets.":
    "5. Extend the reusable Rust core with bounded random-access source, streaming/spill writer, and hardened repair/publication APIs.",
}
for old, new in replacements.items():
    if old not in status:
        raise SystemExit(f"status replacement not found: {old[:80]}")
    status = status.replace(old, new, 1)

marker = "### Bounded writer and publication lifecycle\n"
section = """### Reusable Rust successor core

Experiment 0043 promotes the independently parsed microformat into a reusable Rust slice module without allocating Candidate 2. It provides deterministic genesis and replacement-append writers, exact-end strict validation, independently revalidated linked history, report-only bounded suffix recovery, and strict-source `rewrite_all` and `rewrite_selected` operations.

The Rust writer reproduces the established identities exactly:

| Case | Length | SHA-256 |
|---|---:|---|
| Four-object genesis | 16,886 | `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23` |
| Replacement append | 33,550 | `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc` |
| 400-object multi-level genesis | 89,316 | `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70` |

Rewrite creates a new genesis and new byte-scoped commit identity. It performs no semantic dependency discovery, preserves no byte-scoped signatures, and never invokes recovery. Allocation, output, object, page, history, and recovery work are independently bounded.

"""
if marker not in status:
    raise SystemExit("status section marker not found")
status = status.replace(marker, section + marker, 1)
status_path.write_text(status, encoding="utf-8")

evidence_path = Path("docs/PHASE_3_SUCCESSOR_EVIDENCE.md")
evidence = evidence_path.read_text(encoding="utf-8")
blocker = "4. production-language implementations of the immutable successor writer, source reader, recovery, history, and repair paths;"
blocker_new = "4. production random-access/conditional source, streaming or spill-integrated writer, and hardened repair/publication paths beyond the reusable Rust slice core;"
if blocker not in evidence:
    raise SystemExit("evidence blocker not found")
evidence = evidence.replace(blocker, blocker_new, 1)

evidence_marker = "## Bounded deterministic writer and spill lifecycle\n"
evidence_section = """## Reusable Rust successor core

### Experiment 0043 and ADR-0016

A reusable Rust slice module now reproduces the exact four-object genesis, replacement append, and 400-object multi-level identities. It exposes deterministic writing, exact-end strict validation, linked-history verification, bounded suffix recovery, and strict-source full or caller-selected rewrite.

The assurance scopes are intentionally separate:

- current validity never searches for an alternative footer;
- linked history independently revalidates every prefix and fails closed rather than returning a partial chain;
- recovery treats footer magic only as a bounded hint and reports strictly validated prefixes without selecting one;
- rewrite accepts only exact-end strictly validated active state, publishes a new genesis identity, performs no semantic dependency discovery, and does not preserve byte-scoped signatures.

The focused suite covers current-page-count forgery, partial page overlap, ancestor corruption that leaves current validation successful but makes history fail, interrupted publication, recovery attempt/result caps, deterministic selected rewrite, damaged-source rejection, and allocation/output limits.

Three raw/generated/history targets plus one rewrite target extend the immutable-successor fuzz surface. The full cargo-fuzz matrix now contains twenty-five targets.

"""
if evidence_marker not in evidence:
    raise SystemExit("evidence section marker not found")
evidence = evidence.replace(evidence_marker, evidence_section + evidence_marker, 1)

bullet_marker = "- bounded recovery without candidate selection;\n"
if bullet_marker not in evidence:
    raise SystemExit("evidence bullet marker not found")
evidence = evidence.replace(
    bullet_marker,
    bullet_marker + "- reusable Rust successor writing, strict validation, linked history, recovery, rewrite, and fuzz targets;\n",
    1,
)

ref_marker = "- `docs/experiments/0037-exp0002-immutable-successor-vector.md`\n"
if ref_marker not in evidence:
    raise SystemExit("experiment reference marker not found")
evidence = evidence.replace(
    ref_marker,
    ref_marker + "- `docs/experiments/0043-immutable-successor-rust-api.md`\n",
    1,
)
adr_marker = "- `docs/decisions/0015-exp0002-resource-defaults-are-policy.md`\n"
if adr_marker not in evidence:
    raise SystemExit("ADR reference marker not found")
evidence = evidence.replace(
    adr_marker,
    adr_marker + "- `docs/decisions/0016-immutable-successor-rust-assurance-scopes.md`\n",
    1,
)
evidence_path.write_text(evidence, encoding="utf-8")
