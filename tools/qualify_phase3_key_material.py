#!/usr/bin/env python3
"""Fail-closed local preflight for Phase 3 AES/HMAC key files.

This tool never prints key bytes, hashes, or derived identifiers. It checks
only local file/inode/permission/length properties and that the two secrets
are not byte-for-byte equal. Passing this preflight is not key provisioning,
rotation, sealing, or HSM/KMS qualification.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import stat
import sys

KEY_BYTES = 32


class KeyMaterialError(RuntimeError):
    pass


@dataclass(frozen=True)
class KeyFileMetadata:
    role: str
    path: str
    bytes: int
    uid: int
    gid: int
    mode: str
    nlink: int
    device: int
    inode: int
    parent_path: str
    parent_uid: int
    parent_gid: int
    parent_mode: str


def _mode(value: int) -> str:
    return f"{stat.S_IMODE(value):04o}"


def _open_key(path: Path, role: str) -> tuple[bytes, KeyFileMetadata]:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        fd = os.open(path, flags)
    except OSError as exc:
        raise KeyMaterialError(f"{role}: cannot open key file safely: {exc}") from exc
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode):
            raise KeyMaterialError(f"{role}: key must be a regular file")
        if info.st_uid != os.geteuid():
            raise KeyMaterialError(
                f"{role}: key owner uid {info.st_uid} does not match effective uid {os.geteuid()}"
            )
        if info.st_nlink != 1:
            raise KeyMaterialError(f"{role}: key must have exactly one hard link")
        file_mode = stat.S_IMODE(info.st_mode)
        if file_mode & 0o077:
            raise KeyMaterialError(f"{role}: group/world key permissions are not allowed")
        if not file_mode & stat.S_IRUSR:
            raise KeyMaterialError(f"{role}: owner-read permission is required")
        if info.st_size != KEY_BYTES:
            raise KeyMaterialError(
                f"{role}: key must be exactly {KEY_BYTES} bytes, got {info.st_size}"
            )

        data = bytearray()
        while len(data) < KEY_BYTES + 1:
            chunk = os.read(fd, KEY_BYTES + 1 - len(data))
            if not chunk:
                break
            data.extend(chunk)
        if len(data) != KEY_BYTES:
            raise KeyMaterialError(f"{role}: key changed size while being read")

        parent = path.resolve().parent
        parent_info = parent.stat()
        metadata = KeyFileMetadata(
            role=role,
            path=str(path.resolve()),
            bytes=KEY_BYTES,
            uid=info.st_uid,
            gid=info.st_gid,
            mode=_mode(info.st_mode),
            nlink=info.st_nlink,
            device=info.st_dev,
            inode=info.st_ino,
            parent_path=str(parent),
            parent_uid=parent_info.st_uid,
            parent_gid=parent_info.st_gid,
            parent_mode=_mode(parent_info.st_mode),
        )
        return bytes(data), metadata
    finally:
        os.close(fd)


def qualify(aes_path: Path, hmac_path: Path) -> dict:
    aes, aes_metadata = _open_key(aes_path, "aes-256")
    hmac, hmac_metadata = _open_key(hmac_path, "hmac-sha256")

    if (aes_metadata.device, aes_metadata.inode) == (
        hmac_metadata.device,
        hmac_metadata.inode,
    ):
        raise KeyMaterialError("AES and HMAC keys must not be the same file/inode")
    if aes == hmac:
        raise KeyMaterialError("AES and HMAC key material must be distinct")

    return {
        "schema": "ucof-phase3-key-material-preflight-v1",
        "recorded_utc": datetime.now(timezone.utc).isoformat(),
        "effective_uid": os.geteuid(),
        "ok": True,
        "keys": [asdict(aes_metadata), asdict(hmac_metadata)],
        "secret_material_reported": False,
        "claims": {
            "exact_width": True,
            "regular_file": True,
            "effective_uid_owned": True,
            "single_hard_link": True,
            "no_group_or_world_permissions": True,
            "distinct_files": True,
            "distinct_secret_bytes": True,
        },
        "non_claims": {
            "provisioning_qualified": False,
            "rotation_qualified": False,
            "memory_locking_qualified": False,
            "hardware_backing_qualified": False,
            "rollback_qualified": False,
            "secret_zeroization_qualified": False,
        },
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--aes-key", type=Path, required=True)
    parser.add_argument("--hmac-key", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        report = qualify(args.aes_key, args.hmac_key)
    except (OSError, KeyMaterialError) as exc:
        print(f"Phase 3 key-material preflight: FAIL: {exc}", file=sys.stderr)
        return 1

    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    print(encoded, end="")
    if args.output:
        output = args.output if args.output.is_absolute() else Path.cwd() / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
