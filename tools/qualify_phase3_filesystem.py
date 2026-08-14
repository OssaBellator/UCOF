#!/usr/bin/env python3
"""Collect local filesystem evidence for Phase 3 durable private/publication paths.

This is a smoke/qualification harness, not a power-loss proof. It records the
actual Linux mount/filesystem environment and exercises the same classes of
local operations the Phase 3 research implementation relies on: private
permissions, file fsync, directory fsync, no-overwrite hard-link publication,
and unlink + directory fsync.

No network filesystem is silently treated as equivalent to the local Linux
research path. Known network/distributed filesystem types are reported as
unsupported for production claims unless a future provider-specific
qualification record says otherwise.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import sys
import tempfile

NETWORK_FILESYSTEMS = {
    "9p",
    "afs",
    "ceph",
    "cifs",
    "davfs",
    "fuse.sshfs",
    "gcsfuse",
    "glusterfs",
    "lustre",
    "nfs",
    "nfs4",
    "smb3",
    "sshfs",
}


class QualificationError(RuntimeError):
    pass


@dataclass(frozen=True)
class MountInfo:
    mount_point: str
    filesystem_type: str
    mount_options: tuple[str, ...]
    super_options: tuple[str, ...]
    major_minor: str


@dataclass(frozen=True)
class FilesystemCapacity:
    block_size: int
    total_bytes: int
    free_bytes: int
    available_bytes: int
    total_inodes: int
    free_inodes: int
    available_inodes: int


def unescape_mountinfo(value: str) -> str:
    replacements = {
        r"\040": " ",
        r"\011": "\t",
        r"\012": "\n",
        r"\134": "\\",
    }
    for encoded, decoded in replacements.items():
        value = value.replace(encoded, decoded)
    return value


def resolve_mount(path: Path) -> MountInfo:
    resolved = path.resolve()
    candidates: list[tuple[int, MountInfo]] = []
    try:
        lines = Path("/proc/self/mountinfo").read_text().splitlines()
    except OSError as exc:
        raise QualificationError(f"cannot read /proc/self/mountinfo: {exc}") from exc

    for line in lines:
        left, separator, right = line.partition(" - ")
        if not separator:
            continue
        left_fields = left.split()
        right_fields = right.split()
        if len(left_fields) < 6 or len(right_fields) < 3:
            continue
        mount_point = Path(unescape_mountinfo(left_fields[4]))
        try:
            resolved.relative_to(mount_point)
        except ValueError:
            continue
        info = MountInfo(
            mount_point=str(mount_point),
            filesystem_type=right_fields[0],
            mount_options=tuple(sorted(filter(None, left_fields[5].split(",")))),
            super_options=tuple(sorted(filter(None, right_fields[2].split(",")))),
            major_minor=left_fields[2],
        )
        candidates.append((len(str(mount_point)), info))

    if not candidates:
        raise QualificationError(f"cannot resolve mount for {resolved}")
    return max(candidates, key=lambda item: item[0])[1]


def capacity(path: Path) -> FilesystemCapacity:
    stats = os.statvfs(path)
    block_size = stats.f_frsize or stats.f_bsize
    return FilesystemCapacity(
        block_size=block_size,
        total_bytes=stats.f_blocks * block_size,
        free_bytes=stats.f_bfree * block_size,
        available_bytes=stats.f_bavail * block_size,
        total_inodes=stats.f_files,
        free_inodes=stats.f_ffree,
        available_inodes=stats.f_favail,
    )


def fsync_directory(path: Path) -> None:
    flags = os.O_RDONLY
    if hasattr(os, "O_DIRECTORY"):
        flags |= os.O_DIRECTORY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    fd = os.open(path, flags)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def ensure_mode(path: Path, expected: int) -> None:
    actual = stat.S_IMODE(path.stat().st_mode)
    if actual != expected:
        raise QualificationError(
            f"unexpected mode for {path.name}: expected {oct(expected)}, got {oct(actual)}"
        )


def mechanical_smoke(root: Path) -> dict:
    private = root / "private"
    publication = root / "publication"
    private.mkdir(mode=0o700)
    publication.mkdir(mode=0o700)
    ensure_mode(private, 0o700)
    ensure_mode(publication, 0o700)

    source = private / "encrypted-stage.bin"
    payload = (b"UCOF-PHASE3-FILESYSTEM-SMOKE\0" * 64) + os.urandom(32)
    fd = os.open(
        source,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_CLOEXEC", 0),
        0o600,
    )
    try:
        written = 0
        while written < len(payload):
            count = os.write(fd, payload[written:])
            if count <= 0:
                raise QualificationError("short/zero local write")
            written += count
        os.fsync(fd)
    finally:
        os.close(fd)
    ensure_mode(source, 0o600)
    fsync_directory(private)

    destination = publication / "canonical-output.ucof"
    os.link(source, destination, follow_symlinks=False)
    fsync_directory(publication)
    if destination.read_bytes() != payload:
        raise QualificationError("hard-link publication bytes changed")

    overwrite_blocked = False
    try:
        os.link(source, destination, follow_symlinks=False)
    except FileExistsError:
        overwrite_blocked = True
    if not overwrite_blocked:
        raise QualificationError("hard-link publication unexpectedly overwrote destination")

    source_inode = source.stat().st_ino
    destination_inode = destination.stat().st_ino
    if source_inode == 0 or source_inode != destination_inode:
        raise QualificationError("publication hard link does not share source inode")

    source.unlink()
    fsync_directory(private)
    if not destination.exists() or destination.read_bytes() != payload:
        raise QualificationError("published destination did not survive private unlink")

    destination.unlink()
    fsync_directory(publication)

    return {
        "private_mode": "0700",
        "file_mode": "0600",
        "payload_bytes": len(payload),
        "file_fsync": True,
        "private_directory_fsync": True,
        "hard_link_no_overwrite": True,
        "publication_directory_fsync": True,
        "published_inode_equal": True,
        "private_unlink_directory_fsync": True,
        "publication_unlink_directory_fsync": True,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scratch-dir",
        type=Path,
        help="existing directory on the filesystem to qualify; a private temp directory is created below it",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="optional JSON output path; stdout is always written",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if sys.platform != "linux":
        print("Phase 3 filesystem qualification: FAIL: Linux is required", file=sys.stderr)
        return 1

    base = args.scratch_dir.resolve() if args.scratch_dir else Path.cwd().resolve()
    if not base.is_dir():
        print(f"Phase 3 filesystem qualification: FAIL: scratch parent is not a directory: {base}", file=sys.stderr)
        return 1

    temp: Path | None = None
    try:
        mount = resolve_mount(base)
        before = capacity(base)
        temp = Path(tempfile.mkdtemp(prefix=".ucof-phase3-fs-", dir=base))
        os.chmod(temp, 0o700)
        ensure_mode(temp, 0o700)
        smoke = mechanical_smoke(temp)
        after = capacity(base)
        network = mount.filesystem_type.lower() in NETWORK_FILESYSTEMS
        report = {
            "schema": "ucof-phase3-filesystem-smoke-v1",
            "recorded_utc": datetime.now(timezone.utc).isoformat(),
            "platform": {
                "system": platform.system(),
                "release": platform.release(),
                "machine": platform.machine(),
                "python": platform.python_version(),
                "effective_uid": os.geteuid(),
            },
            "mount": asdict(mount),
            "network_or_distributed_filesystem": network,
            "production_policy": (
                "unsupported-without-provider-qualification"
                if network
                else "local-filesystem-mechanical-smoke-only"
            ),
            "capacity_before": asdict(before),
            "capacity_after": asdict(after),
            "mechanical_smoke": smoke,
            "power_loss_qualified": False,
            "anti_rollback_qualified": False,
            "same_uid_unlink_race_closed": False,
            "notes": [
                "Successful fsync calls prove syscall completion, not physical power-loss durability.",
                "Filesystem free-space/inode values are observations, not reservations.",
                "Network/distributed filesystems require separate provider-specific qualification.",
            ],
        }
        encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
        print(encoded, end="")
        if args.output:
            output = args.output if args.output.is_absolute() else Path.cwd() / args.output
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(encoded)
        return 0
    except (OSError, QualificationError) as exc:
        print(f"Phase 3 filesystem qualification: FAIL: {exc}", file=sys.stderr)
        return 1
    finally:
        if temp is not None:
            shutil.rmtree(temp, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
