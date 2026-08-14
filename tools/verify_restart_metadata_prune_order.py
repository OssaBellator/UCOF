#!/usr/bin/env python3
"""Independent proof-order model for terminal restart metadata reclamation.

This tiny model complements the broader restart-metadata compaction model. It
focuses on one crash-safety rule: Terminal is the durable proof that cleanup
completed, so dependent Prepared/source-set metadata must disappear before the
Terminal record may be reclaimed.
"""

from __future__ import annotations

from dataclasses import dataclass
import argparse
import itertools
import random


class ModelError(RuntimeError):
    pass


@dataclass(frozen=True)
class State:
    prepared: bool
    source_set: bool
    terminal: bool

    def valid(self) -> bool:
        # Source-set authority may survive with Terminal after the stage is gone,
        # but source-set without either Prepared or Terminal has lost the cleanup
        # lineage that makes it meaningful.
        if self.source_set and not (self.prepared or self.terminal):
            return False
        # Prepared without Terminal is a live cleanup instruction. Terminal alone
        # is also valid and is the preferred final proof while reclaiming.
        return True


def terminal_reclaim_steps(state: State) -> list[State]:
    if not state.terminal:
        raise ModelError("terminal proof is required before terminal-lineage reclamation")
    current = state
    observed = [current]
    if current.prepared:
        current = State(False, current.source_set, current.terminal)
        observed.append(current)
    if current.source_set:
        current = State(current.prepared, False, current.terminal)
        observed.append(current)
    if current.terminal:
        current = State(current.prepared, current.source_set, False)
        observed.append(current)
    return observed


def resume(state: State) -> State:
    if not state.valid():
        raise ModelError("invalid authenticated metadata graph")
    if state.terminal:
        return terminal_reclaim_steps(state)[-1]
    if state.prepared:
        # Cleanup is not terminal yet; reclamation must not invent completion.
        return state
    return state


def fixed_cases() -> None:
    full = State(True, True, True)
    path = terminal_reclaim_steps(full)
    assert path == [
        State(True, True, True),
        State(False, True, True),
        State(False, False, True),
        State(False, False, False),
    ]
    assert all(state.valid() for state in path)
    for crash_state in path:
        assert resume(crash_state) == State(False, False, False)

    # The dangerous reverse-order states are intentionally rejected or shown to
    # have lost the final proof too early.
    assert not State(False, True, False).valid()
    assert State(True, False, False).valid()
    assert resume(State(True, False, False)) == State(True, False, False)


def exhaustive_subset_check() -> None:
    for prepared, source_set, terminal in itertools.product((False, True), repeat=3):
        state = State(prepared, source_set, terminal)
        if not state.valid():
            continue
        if terminal:
            path = terminal_reclaim_steps(state)
            assert all(item.valid() for item in path)
            assert path[-1] == State(False, False, False)
        else:
            assert resume(state) == state


def randomized_crash_retries(seed: int, campaigns: int) -> int:
    rng = random.Random(seed)
    transitions = 0
    for _ in range(campaigns):
        path = terminal_reclaim_steps(State(True, True, True))
        crash_index = rng.randrange(len(path))
        crashed = path[crash_index]
        transitions += crash_index
        recovered = resume(crashed)
        assert recovered == State(False, False, False)
        transitions += 1
    return transitions


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--campaigns", type=int, default=10000)
    parser.add_argument("--seed", type=int, default=0x179)
    args = parser.parse_args()
    if args.campaigns <= 0:
        parser.error("campaigns must be positive")
    fixed_cases()
    exhaustive_subset_check()
    transitions = randomized_crash_retries(args.seed, args.campaigns)
    print("restart metadata Terminal-last prune-order model: PASS")
    print(f"campaigns={args.campaigns}")
    print(f"modeled_transitions={transitions}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
