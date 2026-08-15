#!/usr/bin/env python3
"""Independent fixed-width nonce-journal capacity model for Phase 3.

This model intentionally does not parse or call the Rust implementation. It
models the resource invariant added around durable nonce-record creation:
configured directory-entry, ordinary-generation, and ordinary-journal-byte
ceilings must reject before a new record appears, while checkpoint compaction
can reclaim ordinary-record generation/byte budget without moving nonce
allocation backward.
"""

from __future__ import annotations

from dataclasses import dataclass

RECORD_BYTES = 128


class CapacityError(RuntimeError):
    pass


@dataclass
class State:
    max_directory_entries: int
    max_generations: int
    max_journal_bytes: int
    durable_generation: int = 0
    next_unreserved: int = 0
    ordinary_records: int = 0
    checkpoints: int = 0

    def directory_entries(self) -> int:
        return self.ordinary_records + self.checkpoints

    def commit(self, lease_size: int) -> tuple[int, int, int]:
        if lease_size <= 0:
            raise CapacityError("lease")
        if self.directory_entries() >= self.max_directory_entries:
            raise CapacityError("directory entry capacity")
        if self.ordinary_records >= self.max_generations:
            raise CapacityError("journal generation capacity")
        if (self.ordinary_records + 1) * RECORD_BYTES > self.max_journal_bytes:
            raise CapacityError("journal byte capacity")

        generation = self.durable_generation + 1
        first = self.next_unreserved
        last = first + lease_size - 1
        self.ordinary_records += 1
        self.durable_generation = generation
        self.next_unreserved = last + 1
        return generation, first, last

    def compact(self) -> None:
        if self.durable_generation == 0:
            raise CapacityError("initial compaction")
        # One transient checkpoint entry may exist before obsolete ordinary
        # records/checkpoints are reclaimed.
        if self.directory_entries() + 1 > self.max_directory_entries + 1:
            raise CapacityError("checkpoint transient")
        self.checkpoints = 1
        self.ordinary_records = 0


def expect_error(fragment: str, fn) -> None:
    before = fn.__self__.__dict__.copy() if getattr(fn, "__self__", None) else None
    try:
        fn()
    except CapacityError as exc:
        if fragment not in str(exc):
            raise AssertionError(f"expected {fragment!r}, got {exc!r}") from exc
    else:
        raise AssertionError(f"expected CapacityError containing {fragment!r}")
    if before is not None and fn.__self__.__dict__ != before:
        raise AssertionError("capacity rejection mutated state")


def test_generation_capacity_and_reclaim() -> None:
    state = State(8, 1, 8 * RECORD_BYTES)
    assert state.commit(5) == (1, 0, 4)
    snapshot = state.__dict__.copy()
    try:
        state.commit(5)
    except CapacityError as exc:
        assert "journal generation capacity" in str(exc)
    else:
        raise AssertionError("generation capacity must reject")
    assert state.__dict__ == snapshot
    state.compact()
    assert state.durable_generation == 1
    assert state.next_unreserved == 5
    assert state.ordinary_records == 0
    assert state.checkpoints == 1
    assert state.commit(5) == (2, 5, 9)


def test_byte_capacity_and_reclaim() -> None:
    state = State(8, 8, RECORD_BYTES)
    assert state.commit(7) == (1, 0, 6)
    snapshot = state.__dict__.copy()
    try:
        state.commit(7)
    except CapacityError as exc:
        assert "journal byte capacity" in str(exc)
    else:
        raise AssertionError("byte capacity must reject")
    assert state.__dict__ == snapshot
    state.compact()
    assert state.commit(7) == (2, 7, 13)


def test_directory_capacity_rejects_before_write() -> None:
    state = State(1, 8, 8 * RECORD_BYTES)
    assert state.commit(3) == (1, 0, 2)
    snapshot = state.__dict__.copy()
    try:
        state.commit(3)
    except CapacityError as exc:
        assert "directory entry capacity" in str(exc)
    else:
        raise AssertionError("directory capacity must reject")
    assert state.__dict__ == snapshot


def test_mixed_cap_campaign() -> None:
    state = State(4, 2, 2 * RECORD_BYTES)
    expected_next = 0
    for cycle in range(64):
        for lease in (cycle % 7 + 1, cycle % 11 + 1):
            generation, first, last = state.commit(lease)
            assert generation == cycle * 2 + (1 if first == expected_next else 2)
            assert first == expected_next
            expected_next = last + 1
        snapshot = state.__dict__.copy()
        try:
            state.commit(1)
        except CapacityError as exc:
            assert "journal generation capacity" in str(exc) or "journal byte capacity" in str(exc)
        else:
            raise AssertionError("mixed exact cap must reject third ordinary record")
        assert state.__dict__ == snapshot
        state.compact()
        assert state.next_unreserved == expected_next
        assert state.ordinary_records == 0
        assert state.checkpoints == 1


def run_all() -> dict[str, int]:
    test_generation_capacity_and_reclaim()
    test_byte_capacity_and_reclaim()
    test_directory_capacity_rejects_before_write()
    test_mixed_cap_campaign()
    return {"fixed_cases": 4, "mixed_cycles": 64, "successful_commits": 128}


def main() -> int:
    summary = run_all()
    print("nonce journal capacity independent model: PASS")
    for key, value in summary.items():
        print(f"{key}={value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
