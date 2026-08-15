#!/usr/bin/env python3
"""Independent journal-entry lifecycle model for Phase 3 restart publication.

The model is intentionally separate from the Rust implementation. It covers
configured journal directory-entry admission, not filesystem inode capacity.
Checkpoint compaction alone may use exactly one transient entry above the
configured bound, and only when the newly-created checkpoint is current.
"""

from __future__ import annotations

from dataclasses import dataclass


class JournalEntryModelError(RuntimeError):
    pass


@dataclass
class State:
    max_entries: int
    ordinary_nonce: int = 1
    manifests: int = 1
    source_sets: int = 1
    checkpoints: int = 1
    prepared: int = 0
    terminal: int = 0
    durable_generation: int = 1
    checkpoint_generation: int = 1

    def entries(self) -> int:
        return (
            self.ordinary_nonce
            + self.manifests
            + self.source_sets
            + self.checkpoints
            + self.prepared
            + self.terminal
        )

    def require_slots(self, count: int, label: str) -> None:
        if count < 0:
            raise JournalEntryModelError("negative slot request")
        if self.entries() + count > self.max_entries:
            raise JournalEntryModelError(f"{label} directory headroom")

    def publish(self) -> None:
        # Fresh nonce + future Prepared must both fit before fresh nonce issuance.
        self.require_slots(2, "compacted publication retirement")
        self.ordinary_nonce += 1
        self.durable_generation += 1

    def prepare_retirement(self) -> None:
        # Recheck immediately before append-only Prepared creation.
        self.require_slots(1, "compacted retirement Prepared")
        self.prepared += 1

    def terminalize(self) -> None:
        if self.prepared != 1:
            raise JournalEntryModelError("Terminal requires Prepared")
        if self.manifests != 1:
            raise JournalEntryModelError("expected one live manifest before cleanup")
        # Stage is outside this journal-directory count. Manifest is removed and
        # directory-synced before Terminal is appended, releasing one entry.
        self.manifests = 0
        self.require_slots(1, "encrypted restart retirement")
        self.terminal += 1

    def compact_terminal_lineage(self) -> None:
        # Current checkpoint may transiently take max+1. A stale checkpoint may
        # not lend this slot; this model always creates the current checkpoint.
        current_entries = self.entries()
        if current_entries > self.max_entries:
            raise JournalEntryModelError("pre-compaction directory over limit")
        transient = current_entries + 1
        if transient > self.max_entries + 1:
            raise JournalEntryModelError("checkpoint transient headroom")
        self.checkpoints += 1
        self.checkpoint_generation = self.durable_generation

        # Reclaim ordinary nonce history, terminal source-set, Prepared,
        # Terminal, then old checkpoint. The current checkpoint survives.
        self.ordinary_nonce = 0
        self.source_sets = 0
        self.prepared = 0
        self.terminal = 0
        self.checkpoints = 1


def run_fixed_cases() -> int:
    cases = 0

    one_short = State(max_entries=5)
    assert one_short.entries() == 4
    try:
        one_short.publish()
    except JournalEntryModelError as exc:
        assert "compacted publication retirement directory headroom" in str(exc)
    else:
        raise AssertionError("one-slot-short publication unexpectedly succeeded")
    assert one_short.entries() == 4
    assert one_short.durable_generation == 1
    cases += 1

    exact = State(max_entries=6)
    assert exact.entries() == 4
    exact.publish()
    assert exact.entries() == 5
    assert exact.durable_generation == 2
    exact.prepare_retirement()
    assert exact.entries() == 6
    exact.terminalize()
    assert exact.entries() == 6  # manifest removed, Terminal added
    exact.compact_terminal_lineage()
    assert exact.entries() == 1
    assert exact.checkpoints == 1
    assert exact.checkpoint_generation == 2
    cases += 1

    recheck = State(max_entries=7)
    recheck.publish()
    assert recheck.entries() == 5
    # Model two unrelated recognized/unknown entries as externally consumed
    # configured capacity. They are not part of lineage reclamation.
    original_entries = recheck.entries
    extra = 2
    recheck.entries = lambda: original_entries() + extra  # type: ignore[method-assign]
    try:
        recheck.prepare_retirement()
    except JournalEntryModelError as exc:
        assert "compacted retirement Prepared directory headroom" in str(exc)
    else:
        raise AssertionError("Prepared recheck did not reject full journal")
    assert recheck.prepared == 0
    cases += 1

    return cases


def main() -> int:
    cases = run_fixed_cases()
    print("Phase 3 journal-entry lifecycle model: PASS")
    print(f"fixed_cases={cases}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
