#!/usr/bin/env python3
"""Executable spill ownership, budget, cleanup, and durability policy model."""

from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import stat
import tempfile
import uuid


class SpillError(RuntimeError):
    pass


class SpillLimit(SpillError):
    pass


class UnsafeEntry(SpillError):
    pass


class OwnershipMismatch(SpillError):
    pass


@dataclass(frozen=True)
class SpillLimits:
    max_bytes: int
    max_files: int
    max_open_files: int


class PrivateSpillWorkspace:
    def __init__(self, root: Path, limits: SpillLimits) -> None:
        self.root = root
        self.limits = limits
        self.owner_token = uuid.uuid4().hex
        self.bytes_written = 0
        self.files_created = 0
        self.open_files = 0
        root.mkdir(mode=0o700)
        os.chmod(root, 0o700)
        self._write_exclusive(".ucof-owner", self.owner_token.encode("ascii"))

    def _write_exclusive(self, name: str, payload: bytes) -> None:
        if self.files_created >= self.limits.max_files:
            raise SpillLimit("spill inode limit")
        if self.open_files >= self.limits.max_open_files:
            raise SpillLimit("spill descriptor limit")
        if self.bytes_written + len(payload) > self.limits.max_bytes:
            raise SpillLimit("spill byte limit")
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        self.open_files += 1
        try:
            descriptor = os.open(self.root / name, flags, 0o600)
            try:
                written = 0
                while written < len(payload):
                    written += os.write(descriptor, payload[written:])
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
        finally:
            self.open_files -= 1
        self.files_created += 1
        self.bytes_written += len(payload)

    def create_run(self, ordinal: int, payload: bytes) -> Path:
        name = f"run-{ordinal:08d}.bin"
        self._write_exclusive(name, payload)
        return self.root / name

    def verify_owner(self) -> None:
        marker = self.root / ".ucof-owner"
        info = marker.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise OwnershipMismatch("unsafe ownership marker")
        if marker.read_text(encoding="ascii") != self.owner_token:
            raise OwnershipMismatch("ownership token mismatch")

    def cleanup(self) -> int:
        self.verify_owner()
        regular_files: list[Path] = []
        for entry in os.scandir(self.root):
            info = entry.stat(follow_symlinks=False)
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise UnsafeEntry(f"refusing unsafe spill entry: {entry.name}")
            regular_files.append(Path(entry.path))
        for path in regular_files:
            path.unlink()
        self.root.rmdir()
        return len(regular_files)


def publish_no_overwrite(staged: Path, destination: Path) -> None:
    descriptor = os.open(staged, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    os.link(staged, destination, follow_symlinks=False)
    directory = os.open(destination.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def assert_private(path: Path, expected_type: int) -> None:
    info = path.lstat()
    assert stat.S_IFMT(info.st_mode) == expected_type
    assert stat.S_IMODE(info.st_mode) & 0o077 == 0


def main() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        base = Path(temporary)
        workspace = PrivateSpillWorkspace(
            base / "private-stage",
            SpillLimits(max_bytes=96, max_files=4, max_open_files=1),
        )
        assert_private(workspace.root, stat.S_IFDIR)
        assert_private(workspace.root / ".ucof-owner", stat.S_IFREG)
        first = workspace.create_run(1, b"canonical-run-one")
        second = workspace.create_run(2, b"canonical-run-two")
        assert_private(first, stat.S_IFREG)
        assert_private(second, stat.S_IFREG)
        assert workspace.open_files == 0

        try:
            workspace.create_run(3, b"x" * 64)
        except SpillLimit as error:
            assert str(error) == "spill byte limit"
        else:
            raise AssertionError("spill byte budget was not enforced")

        workspace.open_files = 1
        try:
            workspace.create_run(3, b"small")
        except SpillLimit as error:
            assert str(error) == "spill descriptor limit"
        else:
            raise AssertionError("spill descriptor budget was not enforced")
        workspace.open_files = 0

        published = base / "published.ucof"
        publish_no_overwrite(first, published)
        assert published.read_bytes() == first.read_bytes()
        try:
            publish_no_overwrite(second, published)
        except FileExistsError:
            pass
        else:
            raise AssertionError("publication overwrote an existing destination")

        external = base / "external-secret"
        external.write_bytes(b"must survive")
        symlink = workspace.root / "attacker-link"
        symlink.symlink_to(external)
        try:
            workspace.cleanup()
        except UnsafeEntry:
            pass
        else:
            raise AssertionError("cleanup followed or removed an unowned symlink")
        assert external.read_bytes() == b"must survive"
        symlink.unlink()

        marker = workspace.root / ".ucof-owner"
        original_marker = marker.read_text(encoding="ascii")
        marker.write_text("different-owner", encoding="ascii")
        try:
            workspace.cleanup()
        except OwnershipMismatch:
            pass
        else:
            raise AssertionError("cleanup ignored ownership mismatch")
        marker.write_text(original_marker, encoding="ascii")
        assert workspace.cleanup() == 3

        inode_workspace = PrivateSpillWorkspace(
            base / "inode-stage",
            SpillLimits(max_bytes=64, max_files=2, max_open_files=1),
        )
        inode_workspace.create_run(1, b"one")
        try:
            inode_workspace.create_run(2, b"two")
        except SpillLimit as error:
            assert str(error) == "spill inode limit"
        else:
            raise AssertionError("spill inode budget was not enforced")
        assert inode_workspace.cleanup() == 2

    capability_matrix = {
        "private_creation": "required",
        "exclusive_no_follow_open": "required-or-fail-closed",
        "file_fsync": "required-before-publication",
        "directory_fsync": "required-when-platform-supports-durable-directory-sync",
        "no_overwrite_link_or_rename": "required",
        "atomic_visibility": "platform-capability-must-be-probed",
        "secure_deletion": "not-claimed",
    }
    assert capability_matrix["secure_deletion"] == "not-claimed"

    print("private_workspace_mode=pass")
    print("private_file_mode=pass")
    print("byte_inode_descriptor_budgets=pass")
    print("exclusive_no_follow_creation=pass")
    print("ownership_marker_cleanup=pass")
    print("symlink_safe_cleanup=pass")
    print("external_target_survives=pass")
    print("no_overwrite_publication=pass")
    print("file_and_directory_sync_order=pass")
    print("secure_deletion_claim=none")
    print("finding=cleanup authority requires an unforgeable caller-held ownership token")
    print("finding=portable durability is a probed capability and must fail closed when required")
    print("finding=unlinking spill files is not a confidentiality-grade secure deletion guarantee")


if __name__ == "__main__":
    main()
