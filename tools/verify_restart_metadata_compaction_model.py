#!/usr/bin/env python3
"""Independent Phase 3 restart-metadata compaction state model.

This intentionally does not parse or call the Rust implementation. It models
only the safety invariants Experiment 0179 claims: monotonic nonce authority,
checkpoint-before-prune ordering, live-stage verification authority,
retirement reclamation, rollback boundaries, and exact private-metadata quota
admission.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import argparse
import random
from typing import Dict, Optional, Tuple

NONCE_BYTES = 128
CHECKPOINT_BYTES = 112
RETIREMENT_BYTES = 208
SOURCE_SET_BYTES = 176


class ModelError(RuntimeError):
    pass


@dataclass(frozen=True)
class NonceRecord:
    generation: int
    first: int
    last: int
    next_unreserved: int


@dataclass(frozen=True)
class Checkpoint:
    generation: int
    next_unreserved: int


@dataclass(frozen=True)
class Manifest:
    generation: int
    operation: int
    stage_identity: int
    object_count: int


@dataclass(frozen=True)
class SourceSet:
    generation: int
    operation: int
    stage_identity: int
    object_count: int
    source_identity: int


@dataclass(frozen=True)
class Retirement:
    crashed: int
    fresh: int
    payload_identity: int
    stage_identity: int


@dataclass
class State:
    nonce_records: Dict[int, NonceRecord] = field(default_factory=dict)
    checkpoints: Dict[int, Checkpoint] = field(default_factory=dict)
    checkpoint_directory_synced: set[int] = field(default_factory=set)
    manifests: Dict[int, Manifest] = field(default_factory=dict)
    source_sets: Dict[int, SourceSet] = field(default_factory=dict)
    prepared: Dict[Tuple[int, int], Retirement] = field(default_factory=dict)
    terminal: Dict[Tuple[int, int], Retirement] = field(default_factory=dict)

    def clone_signature(self) -> tuple:
        return (
            tuple(sorted(self.nonce_records.items())),
            tuple(sorted(self.checkpoints.items())),
            tuple(sorted(self.checkpoint_directory_synced)),
            tuple(sorted(self.manifests.items())),
            tuple(sorted(self.source_sets.items())),
            tuple(sorted(self.prepared.items())),
            tuple(sorted(self.terminal.items())),
        )

    def persistent_bytes(self) -> int:
        return (
            len(self.nonce_records) * NONCE_BYTES
            + len(self.checkpoints) * CHECKPOINT_BYTES
            + len(self.prepared) * RETIREMENT_BYTES
            + len(self.terminal) * RETIREMENT_BYTES
            + len(self.source_sets) * SOURCE_SET_BYTES
        )

    @staticmethod
    def at_least(previous: int, current: int) -> bool:
        return current >= previous

    def scan(self, trusted_floor: Optional[Tuple[int, int]] = None) -> Tuple[int, int]:
        checkpoints = sorted(self.checkpoints.values(), key=lambda c: c.generation)
        for previous, current in zip(checkpoints, checkpoints[1:]):
            if current.generation <= previous.generation:
                raise ModelError("checkpoint generation rollback")
            if not self.at_least(previous.next_unreserved, current.next_unreserved):
                raise ModelError("checkpoint counter rollback")

        if checkpoints:
            selected = checkpoints[-1]
            generation = selected.generation
            next_unreserved = selected.next_unreserved
            for record in self.nonce_records.values():
                if record.generation <= selected.generation:
                    if record.next_unreserved > selected.next_unreserved:
                        raise ModelError("checkpoint below historical nonce record")
                    if (
                        record.generation == selected.generation
                        and record.next_unreserved != selected.next_unreserved
                    ):
                        raise ModelError("checkpoint disagrees with same-generation record")
        else:
            generation = 0
            next_unreserved = 0

        for record in sorted(self.nonce_records.values(), key=lambda r: r.generation):
            if record.generation <= generation:
                continue
            if record.generation != generation + 1:
                raise ModelError("post-checkpoint generation gap")
            if record.first != next_unreserved:
                raise ModelError("post-checkpoint nonce gap")
            if record.last < record.first or record.next_unreserved != record.last + 1:
                raise ModelError("invalid nonce record")
            generation = record.generation
            next_unreserved = record.next_unreserved

        if trusted_floor is not None:
            floor_generation, floor_next = trusted_floor
            if generation < floor_generation or next_unreserved < floor_next:
                raise ModelError("below trusted floor")
        return generation, next_unreserved

    def commit(self, lease_size: int) -> NonceRecord:
        if lease_size <= 0:
            raise ModelError("invalid lease size")
        generation, next_unreserved = self.scan()
        record = NonceRecord(
            generation=generation + 1,
            first=next_unreserved,
            last=next_unreserved + lease_size - 1,
            next_unreserved=next_unreserved + lease_size,
        )
        if record.generation in self.nonce_records:
            raise ModelError("generation already exists")
        self.nonce_records[record.generation] = record
        return record

    def add_live_restart(self, generation: int, object_count: int, source_identity: int) -> None:
        if generation not in self.nonce_records:
            raise ModelError("live restart lacks original nonce record")
        manifest = Manifest(
            generation=generation,
            operation=1000 + generation,
            stage_identity=2000 + generation,
            object_count=object_count,
        )
        self.manifests[generation] = manifest
        self.source_sets[generation] = SourceSet(
            generation=generation,
            operation=manifest.operation,
            stage_identity=manifest.stage_identity,
            object_count=object_count,
            source_identity=source_identity,
        )

    def add_prepared(self, crashed: int, fresh: int, payload_identity: int) -> None:
        current_generation, _ = self.scan()
        if fresh != current_generation or fresh <= crashed:
            raise ModelError("prepared retirement not at current fresh generation")
        manifest = self.manifests.get(crashed)
        if manifest is None:
            raise ModelError("prepared retirement lacks live crashed manifest")
        self.prepared[(crashed, fresh)] = Retirement(
            crashed,
            fresh,
            payload_identity,
            manifest.stage_identity,
        )

    def terminalize(self, crashed: int, fresh: int) -> None:
        pair = (crashed, fresh)
        prepared = self.prepared.get(pair)
        if prepared is None:
            raise ModelError("terminal without prepared")
        self.manifests.pop(crashed, None)
        self.terminal[pair] = prepared

    def validate_graph(self, current_generation: int) -> None:
        fresh_by_crashed: Dict[int, int] = {}
        for crashed, fresh in tuple(self.prepared) + tuple(self.terminal):
            prior = fresh_by_crashed.setdefault(crashed, fresh)
            if prior != fresh:
                raise ModelError("competing retirement generations")
            if crashed > current_generation or fresh > current_generation:
                raise ModelError("retirement ahead of nonce authority")

        for pair in set(self.prepared).intersection(self.terminal):
            if self.prepared[pair] != self.terminal[pair]:
                raise ModelError("prepared/terminal payload mismatch")

        terminal_crashed = {crashed for crashed, _ in self.terminal}
        prepared_crashed = {crashed for crashed, _ in self.prepared}
        if terminal_crashed.intersection(self.manifests):
            raise ModelError("terminal retirement retains live manifest")

        for generation, manifest in self.manifests.items():
            if generation > current_generation:
                raise ModelError("manifest ahead of nonce authority")
            if generation not in self.nonce_records:
                raise ModelError("live manifest lacks original nonce record")
            source = self.source_sets.get(generation)
            if source is not None:
                if (
                    source.operation != manifest.operation
                    or source.stage_identity != manifest.stage_identity
                    or source.object_count != manifest.object_count
                ):
                    raise ModelError("source-set/live-manifest mismatch")

        for generation, source in self.source_sets.items():
            if generation > current_generation:
                raise ModelError("source-set ahead of nonce authority")
            if source.source_identity == 0:
                raise ModelError("zero source identity")
            manifest = self.manifests.get(generation)
            if manifest is not None:
                continue
            if generation not in prepared_crashed and generation not in terminal_crashed:
                raise ModelError("source-set authority without live restart or cleanup")
            retirement = next(
                (
                    record
                    for pair, record in {**self.prepared, **self.terminal}.items()
                    if pair[0] == generation
                ),
                None,
            )
            if retirement is None:
                raise ModelError("source-set cleanup authority missing")
            if source.stage_identity != retirement.stage_identity:
                raise ModelError("source-set/retirement mismatch")

    def protected_generations(self) -> set[int]:
        # The ordinary generation record is retained only while a live stage
        # manifest still needs it for exact encrypted-stage lease verification.
        # A checkpoint is sufficient global nonce-history authority otherwise.
        return set(self.manifests)

    def compaction_required_before_prune(self) -> int:
        generation, _ = self.scan()
        if generation == 0:
            raise ModelError("cannot compact initial state")
        extra = 0 if generation in self.checkpoints else CHECKPOINT_BYTES
        return self.persistent_bytes() + extra

    def compact(
        self,
        *,
        cut: str = "complete",
        quota: Optional[int] = None,
        trusted_floor: Optional[Tuple[int, int]] = None,
    ) -> dict:
        generation, next_unreserved = self.scan(trusted_floor)
        if generation == 0:
            raise ModelError("cannot compact initial state")
        self.validate_graph(generation)
        required = self.compaction_required_before_prune()
        if quota is not None and quota < required:
            raise ModelError("private storage limit")

        checkpoint = Checkpoint(generation, next_unreserved)
        existing = self.checkpoints.get(generation)
        if existing is not None and existing != checkpoint:
            raise ModelError("checkpoint conflict")
        self.checkpoints[generation] = checkpoint

        if cut == "after_file_sync":
            return {
                "generation": generation,
                "required": required,
                "pruned_nonce": 0,
                "preserved_nonce": len(self.protected_generations()),
            }

        self.checkpoint_directory_synced.add(generation)
        if cut == "after_directory_sync":
            return {
                "generation": generation,
                "required": required,
                "pruned_nonce": 0,
                "preserved_nonce": len(self.protected_generations()),
            }

        if generation not in self.checkpoint_directory_synced:
            raise AssertionError("prune attempted without directory-synced checkpoint")

        protected = self.protected_generations()
        nonce_prune = [
            record_generation
            for record_generation in self.nonce_records
            if record_generation <= generation and record_generation not in protected
        ]
        for record_generation in nonce_prune:
            del self.nonce_records[record_generation]

        terminal_pairs = set(self.terminal)
        terminal_crashed = {crashed for crashed, _ in terminal_pairs}
        source_pruned = 0
        for source_generation in list(self.source_sets):
            if source_generation in terminal_crashed:
                del self.source_sets[source_generation]
                source_pruned += 1

        if cut == "after_source_prune":
            return {
                "generation": generation,
                "required": required,
                "pruned_nonce": len(nonce_prune),
                "preserved_nonce": len(protected.intersection(self.nonce_records)),
                "pruned_retirement": 0,
                "pruned_source": source_pruned,
                "pruned_checkpoints": 0,
            }

        retirement_pruned = 0
        for pair in list(self.prepared):
            if pair in terminal_pairs:
                del self.prepared[pair]
                retirement_pruned += 1

        if cut == "after_prepared_prune":
            return {
                "generation": generation,
                "required": required,
                "pruned_nonce": len(nonce_prune),
                "preserved_nonce": len(protected.intersection(self.nonce_records)),
                "pruned_retirement": retirement_pruned,
                "pruned_source": source_pruned,
                "pruned_checkpoints": 0,
            }

        for pair in list(self.terminal):
            del self.terminal[pair]
            retirement_pruned += 1

        old_checkpoints = [g for g in self.checkpoints if g < generation]
        for old in old_checkpoints:
            del self.checkpoints[old]
            self.checkpoint_directory_synced.discard(old)

        report = {
            "generation": generation,
            "required": required,
            "pruned_nonce": len(nonce_prune),
            "preserved_nonce": len(protected.intersection(self.nonce_records)),
            "pruned_retirement": retirement_pruned,
            "pruned_source": source_pruned,
            "pruned_checkpoints": len(old_checkpoints),
        }
        if cut == "after_prune":
            return report
        if cut != "complete":
            raise ModelError(f"unknown cut: {cut}")
        return report

    def retry_live_restart(self, crashed_generation: int, lease_size: int) -> NonceRecord:
        if crashed_generation not in self.manifests:
            raise ModelError("missing live manifest")
        if crashed_generation not in self.source_sets:
            raise ModelError("missing source-set authority")
        if crashed_generation not in self.nonce_records:
            raise ModelError("missing original crashed nonce record")
        current_generation, current_next = self.scan()
        crashed = self.nonce_records[crashed_generation]
        if current_generation < crashed_generation or current_next < crashed.next_unreserved:
            raise ModelError("global authority below crashed stage")
        return self.commit(lease_size)


def expect_error(fragment: str, fn) -> None:
    try:
        fn()
    except ModelError as exc:
        if fragment not in str(exc):
            raise AssertionError(f"expected {fragment!r}, got {exc!r}") from exc
    else:
        raise AssertionError(f"expected ModelError containing {fragment!r}")


def test_checkpoint_retry_ordering() -> None:
    state = State()
    state.commit(5)
    state.commit(7)
    before = state.clone_signature()
    result = state.compact(cut="after_file_sync")
    assert result["generation"] == 2
    assert set(state.nonce_records) == {1, 2}
    assert 2 in state.checkpoints
    assert 2 not in state.checkpoint_directory_synced
    assert before[0] == state.clone_signature()[0]
    result = state.compact()
    assert result["pruned_nonce"] == 2
    assert state.scan() == (2, 12)
    assert 2 in state.checkpoint_directory_synced


def test_checkpoint_chain_rollback() -> None:
    state = State()
    state.commit(5)
    state.commit(7)
    state.checkpoints[2] = Checkpoint(2, 12)
    state.checkpoints[3] = Checkpoint(3, 11)
    expect_error("checkpoint counter rollback", state.scan)


def test_graph_fail_closed() -> None:
    state = State()
    state.commit(10)
    state.add_live_restart(1, 5, 99)
    state.manifests.clear()
    expect_error("source-set authority without live restart or cleanup", state.compact)

    state = State()
    state.commit(10)
    state.add_live_restart(1, 5, 99)
    state.commit(4)
    state.add_prepared(1, 2, 123)
    state.terminalize(1, 2)
    state.manifests[1] = Manifest(1, 1001, 2001, 5)
    expect_error("terminal retirement retains live manifest", state.compact)


def test_prepared_then_terminal_reclamation() -> None:
    state = State()
    state.commit(10)
    state.add_live_restart(1, 5, 99)
    state.commit(6)
    state.add_prepared(1, 2, 123)
    first = state.compact()
    assert first["pruned_nonce"] == 1
    assert first["preserved_nonce"] == 1
    assert set(state.nonce_records) == {1}
    assert (1, 2) in state.prepared
    assert 1 in state.source_sets
    assert state.scan() == (2, 16)

    state.terminalize(1, 2)
    second = state.compact()
    assert second["pruned_nonce"] == 1
    assert second["pruned_retirement"] == 2
    assert second["pruned_source"] == 1
    assert state.scan() == (2, 16)


def test_source_prune_cut_is_retryable() -> None:
    state = State()
    state.commit(10)
    state.add_live_restart(1, 5, 99)
    state.commit(6)
    state.add_prepared(1, 2, 123)
    state.terminalize(1, 2)

    cut = state.compact(cut="after_source_prune")
    assert cut["pruned_nonce"] == 2
    assert cut["pruned_source"] == 1
    assert cut["pruned_retirement"] == 0
    assert state.source_sets == {}
    assert (1, 2) in state.prepared
    assert (1, 2) in state.terminal
    assert state.scan() == (2, 16)

    retry = state.compact()
    assert retry["pruned_nonce"] == 0
    assert retry["pruned_source"] == 0
    assert retry["pruned_retirement"] == 2
    assert state.prepared == {}
    assert state.terminal == {}
    assert state.scan() == (2, 16)


def test_prepared_prune_cut_keeps_terminal_completion_authority() -> None:
    state = State()
    state.commit(10)
    state.add_live_restart(1, 5, 99)
    state.commit(6)
    state.add_prepared(1, 2, 123)
    state.terminalize(1, 2)

    cut = state.compact(cut="after_prepared_prune")
    assert cut["pruned_nonce"] == 2
    assert cut["pruned_source"] == 1
    assert cut["pruned_retirement"] == 1
    assert state.source_sets == {}
    assert state.prepared == {}
    assert (1, 2) in state.terminal
    assert state.scan() == (2, 16)

    retry = state.compact()
    assert retry["pruned_nonce"] == 0
    assert retry["pruned_source"] == 0
    assert retry["pruned_retirement"] == 1
    assert state.terminal == {}
    assert state.scan() == (2, 16)


def test_burn_compact_retry_terminal_lifecycle() -> None:
    state = State()
    first = state.commit(20)
    assert first.generation == 1
    state.add_live_restart(1, 11, 0x53)
    state.compact()
    assert set(state.nonce_records) == {1}

    burned = state.commit(13)
    assert burned.generation == 2
    second_compaction = state.compact()
    assert second_compaction["pruned_nonce"] == 1
    assert second_compaction["preserved_nonce"] == 1
    assert set(state.nonce_records) == {1}
    assert state.scan() == (2, burned.next_unreserved)

    retried = state.retry_live_restart(1, 13)
    assert retried.generation == 3
    assert retried.first == burned.next_unreserved
    state.add_prepared(1, 3, 0x9001)

    prepared_compaction = state.compact()
    assert prepared_compaction["pruned_nonce"] == 1
    assert prepared_compaction["preserved_nonce"] == 1
    assert set(state.nonce_records) == {1}
    assert state.scan() == (3, retried.next_unreserved)

    state.terminalize(1, 3)
    final = state.compact()
    assert final["pruned_nonce"] == 1
    assert final["pruned_retirement"] == 2
    assert final["pruned_source"] == 1
    assert final["pruned_checkpoints"] == 0
    assert state.scan() == (3, retried.next_unreserved)
    assert state.nonce_records == {}
    assert list(state.checkpoints) == [3]
    assert state.source_sets == {}
    assert state.prepared == {}
    assert state.terminal == {}


def test_trusted_floor_boundary() -> None:
    state = State()
    state.commit(5)
    state.commit(7)
    state.compact()
    del state.checkpoints[2]
    state.checkpoint_directory_synced.discard(2)
    assert state.scan() == (0, 0)
    expect_error("below trusted floor", lambda: state.scan((2, 12)))


def test_exact_quota() -> None:
    state = State()
    state.commit(5)
    state.commit(7)
    required = state.compaction_required_before_prune()
    signature = state.clone_signature()
    expect_error("private storage limit", lambda: state.compact(quota=required - 1))
    assert state.clone_signature() == signature
    report = state.compact(quota=required)
    assert report["required"] == required
    assert state.scan() == (2, 12)


def test_repeated_compaction_campaign(seed: int, steps: int) -> None:
    rng = random.Random(seed)
    state = State()
    expected_generation = 0
    expected_next = 0
    for _ in range(1, steps + 1):
        lease = rng.randint(1, 31)
        record = state.commit(lease)
        expected_generation += 1
        assert record.generation == expected_generation
        assert record.first == expected_next
        expected_next += lease
        assert record.next_unreserved == expected_next

        if rng.randrange(4) == 0:
            state.compact()
            assert state.scan() == (expected_generation, expected_next)
            assert len(state.checkpoints) == 1

    state.compact()
    assert state.scan() == (expected_generation, expected_next)
    assert len(state.checkpoints) == 1
    assert len(state.nonce_records) == 0


def test_small_state_matrix() -> None:
    for first in range(1, 5):
        for second in range(1, 5):
            for third in range(1, 5):
                state = State()
                state.commit(first)
                state.commit(second)
                state.compact()
                third_record = state.commit(third)
                assert third_record.generation == 3
                assert third_record.first == first + second
                state.compact()
                assert state.scan() == (3, first + second + third)
                assert list(state.checkpoints) == [3]


def run_all(campaigns: int, steps: int) -> dict:
    test_checkpoint_retry_ordering()
    test_checkpoint_chain_rollback()
    test_graph_fail_closed()
    test_prepared_then_terminal_reclamation()
    test_source_prune_cut_is_retryable()
    test_prepared_prune_cut_keeps_terminal_completion_authority()
    test_burn_compact_retry_terminal_lifecycle()
    test_trusted_floor_boundary()
    test_exact_quota()
    test_small_state_matrix()
    for seed in range(campaigns):
        test_repeated_compaction_campaign(seed, steps)
    return {
        "fixed_cases": 10,
        "matrix_cases": 4 * 4 * 4,
        "campaigns": campaigns,
        "campaign_steps": steps,
        "campaign_transitions": campaigns * steps,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--campaigns", type=int, default=128)
    parser.add_argument("--steps", type=int, default=128)
    args = parser.parse_args()
    if args.campaigns <= 0 or args.steps <= 0:
        parser.error("campaigns and steps must be positive")
    summary = run_all(args.campaigns, args.steps)
    print("restart metadata compaction independent model: PASS")
    for key, value in summary.items():
        print(f"{key}={value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
