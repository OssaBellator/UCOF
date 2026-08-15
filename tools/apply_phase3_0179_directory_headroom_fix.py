#!/usr/bin/env python3
"""Apply/check the two pending Experiment 0179 directory-headroom fixes.

This exists because the current development/acceptance path is local rather
than GitHub Actions and some constrained environments cannot safely patch the
large Rust source through an editor. The transformation is intentionally
narrow: it refuses source drift, partial application, or ambiguous matches.

The two fixes are:

1. saturate the defensive ``max_directory_entries + 1`` scan ceiling at
   ``usize::MAX`` rather than overflowing;
2. when scanning exactly one entry above the configured directory limit,
   require the newest authenticated checkpoint to cover the *current recovered
   durable generation*. An older checkpoint may not lend transient headroom to
   newer post-checkpoint state.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCE = (
    ROOT
    / "crates/ucof-experiments/src/immutable_successor/"
    "bounded_end_to_end_candidate/restart_metadata_compaction.rs"
)

OLD_CEILING = """fn compacted_directory_scan_ceiling(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<usize> {
    journal
        .limits
        .max_directory_entries
        .checked_add(1)
        .ok_or_else(|| \"compacted directory entry ceiling\".to_owned())
}
"""

NEW_CEILING = """fn compacted_directory_scan_ceiling(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<usize> {
    Ok(journal.limits.max_directory_entries.saturating_add(1))
}
"""

OLD_POST_REPLAY = """        let mut journal_records = 0usize;
        for record in records {
            if record.generation <= durable.generation {
                continue;
            }
            if durable.generation.checked_add(1) != Some(record.generation)
                || durable.next_unreserved != Some(record.lease_first)
            {
                return Err(\"compacted nonce generation/lease gap\".into());
            }
            durable = DurableNonceState {
                generation: record.generation,
                next_unreserved: record.next_unreserved,
            };
            journal_records = journal_records
                .checked_add(1)
                .ok_or_else(|| \"compacted nonce journal record count\".to_owned())?;
        }
        if let Some(floor) = trusted_floor {
"""

NEW_POST_REPLAY = """        let mut journal_records = 0usize;
        for record in records {
            if record.generation <= durable.generation {
                continue;
            }
            if durable.generation.checked_add(1) != Some(record.generation)
                || durable.next_unreserved != Some(record.lease_first)
            {
                return Err(\"compacted nonce generation/lease gap\".into());
            }
            durable = DurableNonceState {
                generation: record.generation,
                next_unreserved: record.next_unreserved,
            };
            journal_records = journal_records
                .checked_add(1)
                .ok_or_else(|| \"compacted nonce journal record count\".to_owned())?;
        }
        if directory_entries > self.journal.limits.max_directory_entries
            && checkpoint_generation != Some(durable.generation)
        {
            return Err(\"compacted nonce stale checkpoint headroom\".into());
        }
        if let Some(floor) = trusted_floor {
"""


class PatchError(RuntimeError):
    pass


@dataclass(frozen=True)
class PatchState:
    ceiling_fixed: bool
    stale_headroom_fixed: bool

    @property
    def complete(self) -> bool:
        return self.ceiling_fixed and self.stale_headroom_fixed

    @property
    def untouched(self) -> bool:
        return not self.ceiling_fixed and not self.stale_headroom_fixed


def _count(text: str, needle: str) -> int:
    return text.count(needle)


def inspect_source(text: str) -> PatchState:
    old_ceiling = _count(text, OLD_CEILING)
    new_ceiling = _count(text, NEW_CEILING)
    old_replay = _count(text, OLD_POST_REPLAY)
    new_replay = _count(text, NEW_POST_REPLAY)

    if old_ceiling + new_ceiling != 1:
        raise PatchError(
            "0179 directory-ceiling source shape is ambiguous or has drifted"
        )
    if old_replay + new_replay != 1:
        raise PatchError(
            "0179 post-replay source shape is ambiguous or has drifted"
        )

    state = PatchState(
        ceiling_fixed=new_ceiling == 1,
        stale_headroom_fixed=new_replay == 1,
    )
    if not state.complete and not state.untouched:
        raise PatchError(
            "0179 directory-headroom fix is only partially applied; refuse to guess"
        )
    return state


def apply_text(text: str) -> tuple[str, PatchState]:
    state = inspect_source(text)
    if state.complete:
        return text, state
    updated = text.replace(OLD_CEILING, NEW_CEILING, 1)
    updated = updated.replace(OLD_POST_REPLAY, NEW_POST_REPLAY, 1)
    final = inspect_source(updated)
    if not final.complete:
        raise AssertionError("0179 directory-headroom transformation did not complete")
    return updated, final


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument(
        "--apply",
        action="store_true",
        help="write the exact transformation; default mode only checks state",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    source = args.source if args.source.is_absolute() else ROOT / args.source
    try:
        text = source.read_text()
        state = inspect_source(text)
        if args.apply:
            updated, final = apply_text(text)
            if updated != text:
                source.write_text(updated)
                print(f"APPLIED {source}")
            else:
                print(f"ALREADY-APPLIED {source}")
            print(
                "ceiling_fixed=true stale_checkpoint_headroom_fixed=true"
                if final.complete
                else "unexpected incomplete state"
            )
            return 0

        if state.complete:
            print("Experiment 0179 directory-headroom source: FIXED")
            return 0
        print(
            "Experiment 0179 directory-headroom source: PENDING; "
            "run with --apply from a clean checkout",
            file=sys.stderr,
        )
        return 1
    except (OSError, PatchError) as exc:
        print(f"Experiment 0179 directory-headroom patch: FAIL: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
