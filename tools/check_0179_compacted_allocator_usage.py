#!/usr/bin/env python3
"""Fail closed if 0179 compacted flows bypass checkpoint-aware nonce authority."""

from __future__ import annotations

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "crates/ucof-experiments/src/immutable_successor/bounded_end_to_end_candidate"

FILES = {
    "compacted_source_bound_restart.rs": (
        "CompactedNonceJournal::new(journal)",
        "compacted.recover_authority(trusted_floor)",
        "compacted.commit_descriptor_session(",
    ),
    "restart_metadata_compaction.rs": (
        "struct CompactedNonceJournal<'a>",
        "self.scan(None)?.durable",
        "self.journal.persist_record(record, cut)",
    ),
}

FORBIDDEN = (
    "journal.recover_authority(",
    "journal.commit_descriptor_session(",
)


class UsageError(RuntimeError):
    pass


def verify() -> dict[str, int]:
    checked = 0
    for name, required in FILES.items():
        path = BASE / name
        if not path.is_file():
            raise UsageError(f"required 0179 allocation source is missing: {path.relative_to(ROOT)}")
        source = path.read_text()
        missing = [token for token in required if token not in source]
        if missing:
            raise UsageError(
                f"checkpoint-aware allocation tokens missing in {name}: {', '.join(missing)}"
            )
        present_forbidden = [token for token in FORBIDDEN if token in source]
        if present_forbidden:
            raise UsageError(
                f"legacy nonce allocation bypass in {name}: {', '.join(present_forbidden)}"
            )
        checked += 1
    return {"files_checked": checked, "forbidden_patterns": len(FORBIDDEN)}


def main() -> int:
    try:
        summary = verify()
    except UsageError as exc:
        print(f"0179 compacted allocator usage: FAIL: {exc}", file=sys.stderr)
        return 1
    print("0179 compacted allocator usage: PASS")
    for key, value in summary.items():
        print(f"{key}={value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
