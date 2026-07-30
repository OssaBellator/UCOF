#!/usr/bin/env python3
"""Compare rollback detection across internal and external freshness models."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


class Decision(Enum):
    ACCEPT = "accept"
    REJECT_ROLLBACK = "reject-rollback"
    REJECT_FORK = "reject-fork"
    REQUIRE_ONLINE_PROOF = "require-online-proof"


@dataclass(frozen=True)
class Version:
    sequence: int
    snapshot_digest: str
    commit_digest: str


V0 = Version(0, "snapshot-0", "commit-0")
V1 = Version(1, "snapshot-1", "commit-1")
V2 = Version(2, "snapshot-2", "commit-2")
FORK2 = Version(2, "snapshot-fork-2", "commit-fork-2")


@dataclass
class TrustedState:
    latest: Version | None = None

    def observe(self, version: Version) -> Decision:
        if self.latest is None:
            self.latest = version
            return Decision.ACCEPT
        if version.sequence < self.latest.sequence:
            return Decision.REJECT_ROLLBACK
        if version.sequence == self.latest.sequence:
            if version.commit_digest != self.latest.commit_digest:
                return Decision.REJECT_FORK
            return Decision.ACCEPT
        self.latest = version
        return Decision.ACCEPT


@dataclass(frozen=True)
class TransparencyHead:
    latest_sequence: int
    accepted_commit_digests: frozenset[str]


def internal_only(_version: Version) -> Decision:
    # Structural validity, hash integrity, and internal parent links cannot reveal
    # that a complete older file replaced a newer complete file.
    return Decision.ACCEPT


def check_transparency(version: Version, head: TransparencyHead | None) -> Decision:
    if head is None:
        return Decision.REQUIRE_ONLINE_PROOF
    if version.sequence < head.latest_sequence:
        return Decision.REJECT_ROLLBACK
    if version.sequence == head.latest_sequence and version.commit_digest not in head.accepted_commit_digests:
        return Decision.REJECT_FORK
    if version.sequence > head.latest_sequence:
        return Decision.REQUIRE_ONLINE_PROOF
    return Decision.ACCEPT


def main() -> None:
    assert internal_only(V2) is Decision.ACCEPT
    assert internal_only(V0) is Decision.ACCEPT

    tofu = TrustedState()
    assert tofu.observe(V0) is Decision.ACCEPT  # first-use rollback is invisible
    assert tofu.observe(V2) is Decision.ACCEPT
    assert tofu.observe(V1) is Decision.REJECT_ROLLBACK
    assert tofu.observe(FORK2) is Decision.REJECT_FORK

    preprovisioned = TrustedState(latest=V2)
    assert preprovisioned.observe(V0) is Decision.REJECT_ROLLBACK
    assert preprovisioned.observe(V2) is Decision.ACCEPT

    # Two devices with unsynchronized trusted state can make different decisions.
    device_a = TrustedState(latest=V2)
    device_b = TrustedState(latest=V0)
    assert device_a.observe(V1) is Decision.REJECT_ROLLBACK
    assert device_b.observe(V1) is Decision.ACCEPT

    head = TransparencyHead(2, frozenset({V2.commit_digest}))
    assert check_transparency(V0, head) is Decision.REJECT_ROLLBACK
    assert check_transparency(FORK2, head) is Decision.REJECT_FORK
    assert check_transparency(V2, head) is Decision.ACCEPT
    assert check_transparency(V2, None) is Decision.REQUIRE_ONLINE_PROOF

    # If accepting V2 is not atomically followed by trusted-state persistence,
    # a crash can leave V0 recorded and a replay of V1 can be accepted.
    stale_after_crash = TrustedState(latest=V0)
    assert stale_after_crash.observe(V1) is Decision.ACCEPT

    print("internal_integrity_accepts_complete_rollback=confirmed")
    print("tofu_first_use_rollback_undetectable=confirmed")
    print("tofu_post_observation_rollback_detection=pass")
    print("same_sequence_fork_detection_with_trusted_digest=pass")
    print("preprovisioned_latest_state_detection=pass")
    print("unsynchronized_device_state_divergence=confirmed")
    print("online_transparency_detection=pass")
    print("offline_transparency_requires_proof=confirmed")
    print("non_atomic_trusted_state_update_window=confirmed")
    print("finding=sequence and internal digests provide integrity ordering, not external freshness")
    print("finding=TOFU protects only after a trusted observation and is device-local unless synchronized")
    print("finding=trusted latest-state updates must be atomic with application acceptance")
    print("finding=online transparency can detect rollback and forks but introduces availability and privacy dependencies")
    print("finding=Phase 3 should expose identities without claiming a freshness mechanism")


if __name__ == "__main__":
    main()
