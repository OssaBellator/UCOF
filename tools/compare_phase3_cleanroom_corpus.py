#!/usr/bin/env python3
"""Compare a Phase 3 reference corpus with clean-room generated output.

This tool intentionally performs only filesystem enumeration and exact byte
hashing. It does not import or execute UCOF encoding/decoding implementation
code, so using it does not teach an independent implementation how to produce
the expected bytes.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys

SCHEMA = "ucof-phase3-cleanroom-corpus-comparison-v1"


class CorpusCompareError(RuntimeError):
    pass


@dataclass(frozen=True)
class FileDigest:
    path: str
    bytes: int
    sha256: str


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def inventory(root: Path) -> dict[str, FileDigest]:
    if not root.is_dir():
        raise CorpusCompareError(f"corpus root is not a directory: {root}")
    result: dict[str, FileDigest] = {}
    for path in sorted(root.rglob("*"), key=lambda item: item.as_posix()):
        if path.is_symlink():
            raise CorpusCompareError(f"symlink corpus entry is forbidden: {path}")
        if not path.is_file():
            continue
        relative = path.relative_to(root).as_posix()
        result[relative] = FileDigest(relative, path.stat().st_size, sha256_file(path))
    if not result:
        raise CorpusCompareError(f"corpus root is empty: {root}")
    return result


def compare(reference: dict[str, FileDigest], candidate: dict[str, FileDigest]) -> dict:
    reference_names = set(reference)
    candidate_names = set(candidate)
    missing = sorted(reference_names - candidate_names)
    extra = sorted(candidate_names - reference_names)
    matched: list[str] = []
    mismatched: list[dict] = []
    for name in sorted(reference_names & candidate_names):
        expected = reference[name]
        actual = candidate[name]
        if expected.bytes == actual.bytes and expected.sha256 == actual.sha256:
            matched.append(name)
        else:
            mismatched.append(
                {
                    "path": name,
                    "reference": asdict(expected),
                    "candidate": asdict(actual),
                }
            )
    return {
        "ok": not missing and not extra and not mismatched,
        "matched": matched,
        "missing": missing,
        "extra": extra,
        "mismatched": mismatched,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        reference_root = args.reference.resolve()
        candidate_root = args.candidate.resolve()
        reference = inventory(reference_root)
        candidate = inventory(candidate_root)
        comparison = compare(reference, candidate)
    except (OSError, CorpusCompareError) as exc:
        print(f"Phase 3 clean-room corpus comparison: FAIL: {exc}", file=sys.stderr)
        return 2

    report = {
        "schema": SCHEMA,
        "recorded_utc": datetime.now(timezone.utc).isoformat(),
        "reference_root": str(reference_root),
        "candidate_root": str(candidate_root),
        "reference_files": len(reference),
        "candidate_files": len(candidate),
        "comparison": comparison,
        "implementation_code_executed": False,
        "interpretation_ambiguity_review_required_separately": True,
    }
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    print(encoded, end="")
    if args.output:
        output = args.output.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(encoded)
    return 0 if comparison["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
