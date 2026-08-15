#!/usr/bin/env python3
"""Observe Phase 3 byte/inode headroom without claiming reservation.

The lifecycle planners compute required private bytes and additional inodes
deterministically. This helper compares supplied requirements with the
filesystem's current statvfs observations plus caller-selected safety margins.
A PASS is only a point-in-time admission observation: unrelated writers can
consume the space/inodes immediately afterward.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import sys


class HeadroomError(RuntimeError):
    pass


@dataclass(frozen=True)
class HeadroomObservation:
    block_size: int
    available_bytes: int
    available_inodes: int
    required_bytes: int
    required_inodes: int
    reserve_bytes: int
    reserve_inodes: int
    byte_headroom_after_requirement: int
    inode_headroom_after_requirement: int
    bytes_ok: bool
    inodes_ok: bool


def observe(
    path: Path,
    required_bytes: int,
    required_inodes: int,
    reserve_bytes: int,
    reserve_inodes: int,
) -> HeadroomObservation:
    for label, value in (
        ("required bytes", required_bytes),
        ("required inodes", required_inodes),
        ("reserve bytes", reserve_bytes),
        ("reserve inodes", reserve_inodes),
    ):
        if value < 0:
            raise HeadroomError(f"{label} must be nonnegative")

    if not path.is_dir():
        raise HeadroomError(f"headroom path is not a directory: {path}")
    stats = os.statvfs(path)
    block_size = stats.f_frsize or stats.f_bsize
    available_bytes = stats.f_bavail * block_size
    available_inodes = stats.f_favail
    byte_headroom = available_bytes - required_bytes
    inode_headroom = available_inodes - required_inodes
    return HeadroomObservation(
        block_size=block_size,
        available_bytes=available_bytes,
        available_inodes=available_inodes,
        required_bytes=required_bytes,
        required_inodes=required_inodes,
        reserve_bytes=reserve_bytes,
        reserve_inodes=reserve_inodes,
        byte_headroom_after_requirement=byte_headroom,
        inode_headroom_after_requirement=inode_headroom,
        bytes_ok=byte_headroom >= reserve_bytes,
        inodes_ok=inode_headroom >= reserve_inodes,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", type=Path, required=True)
    parser.add_argument("--required-bytes", type=int, required=True)
    parser.add_argument("--required-inodes", type=int, default=0)
    parser.add_argument("--reserve-bytes", type=int, default=0)
    parser.add_argument("--reserve-inodes", type=int, default=0)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        observation = observe(
            args.path.resolve(),
            args.required_bytes,
            args.required_inodes,
            args.reserve_bytes,
            args.reserve_inodes,
        )
    except (OSError, HeadroomError) as exc:
        print(f"Phase 3 storage headroom: FAIL: {exc}", file=sys.stderr)
        return 2

    ok = observation.bytes_ok and observation.inodes_ok
    report = {
        "schema": "ucof-phase3-storage-headroom-v1",
        "recorded_utc": datetime.now(timezone.utc).isoformat(),
        "path": str(args.path.resolve()),
        "ok": ok,
        "observation": asdict(observation),
        "reserved": False,
        "race_free": False,
        "notes": [
            "statvfs values are point-in-time observations, not reservations.",
            "Unrelated writers may consume bytes or inodes after this check.",
            "Use the deterministic byte lifecycle planner output as required_bytes rather than estimating here.",
            "Use tools/plan_phase3_private_inodes.py or the deployment bundle's derived value as required_inodes rather than guessing it.",
        ],
    }
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    print(encoded, end="")
    if args.output:
        output = args.output if args.output.is_absolute() else Path.cwd() / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(encoded)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
