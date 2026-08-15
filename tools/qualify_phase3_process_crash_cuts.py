#!/usr/bin/env python3
"""Exercise Phase 3 durability ordering across real process-crash cuts.

This is a filesystem/syscall qualification aid, not a power-loss test. A child
process performs a durability transition and exits with os._exit() at a named
cut. The parent then observes/retries using a fresh process context.

The harness operates only inside a caller-supplied scratch directory and
cleans its unique subdirectory when complete.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
import errno
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

SCHEMA = "ucof-phase3-process-crash-cuts-v1"
CRASH_EXIT = 86
PAYLOAD = b"UCOF phase3 process crash qualification\n" * 64
CHECKPOINT = b"UCOFCP01-process-crash-checkpoint\n"


class CrashCutError(RuntimeError):
    pass


@dataclass
class CaseResult:
    name: str
    ok: bool
    child_returncode: int | None
    observations: dict
    seconds: float


def fsync_directory(path: Path) -> None:
    flags = os.O_RDONLY
    if hasattr(os, "O_DIRECTORY"):
        flags |= os.O_DIRECTORY
    fd = os.open(path, flags)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def write_fsync(path: Path, payload: bytes) -> None:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        view = memoryview(payload)
        while view:
            written = os.write(fd, view)
            if written <= 0:
                raise CrashCutError("short/zero write")
            view = view[written:]
        os.fsync(fd)
    finally:
        os.close(fd)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def worker_checkpoint(root: Path) -> None:
    private = root / "private"
    checkpoint = private / "checkpoint.bin"
    write_fsync(checkpoint, CHECKPOINT)
    # Deliberate crash before private directory fsync.
    os._exit(CRASH_EXIT)


def worker_publication(root: Path) -> None:
    stage = root / "private" / "staged-output.bin"
    destination = root / "publication" / "object.bin"
    os.link(stage, destination)
    # Deliberate crash after link becomes visible but before publication-dir fsync.
    os._exit(CRASH_EXIT)


def worker_retirement(root: Path, cut: str) -> None:
    stage = root / "private" / "restart-stage.bin"
    manifest = root / "journal" / "restart-manifest.bin"
    if stage.exists():
        stage.unlink()
    if cut == "after-stage-unlink":
        os._exit(CRASH_EXIT)
    if manifest.exists():
        manifest.unlink()
    if cut == "after-both-unlinks":
        os._exit(CRASH_EXIT)
    raise CrashCutError(f"unknown retirement worker cut: {cut}")


def child(script: Path, root: Path, scenario: str, cut: str | None = None) -> subprocess.CompletedProcess:
    command = [sys.executable, str(script), "--worker", scenario, "--root", str(root)]
    if cut:
        command += ["--cut", cut]
    return subprocess.run(
        command,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def require_crash(completed: subprocess.CompletedProcess, label: str) -> None:
    if completed.returncode != CRASH_EXIT:
        tail = "\n".join((completed.stderr or completed.stdout).splitlines()[-12:])
        raise CrashCutError(
            f"{label} worker did not reach deliberate crash cut: rc={completed.returncode} {tail}"
        )


def case_checkpoint(script: Path, root: Path) -> CaseResult:
    started = time.monotonic()
    completed = child(script, root, "checkpoint")
    require_crash(completed, "checkpoint")
    checkpoint = root / "private" / "checkpoint.bin"
    if checkpoint.read_bytes() != CHECKPOINT:
        raise CrashCutError("visible file-synced checkpoint bytes changed after process crash")
    # Restart rule: a visible matching checkpoint must still re-sync the pinned
    # directory before it can authorize pruning.
    fsync_directory(root / "private")
    observations = {
        "checkpoint_visible_after_crash": True,
        "checkpoint_bytes_exact": True,
        "retry_directory_fsync": True,
    }
    return CaseResult("checkpoint-file-sync-before-dir-sync", True, completed.returncode, observations, round(time.monotonic()-started, 3))


def case_publication(script: Path, root: Path) -> CaseResult:
    started = time.monotonic()
    private = root / "private"
    publication = root / "publication"
    stage = private / "staged-output.bin"
    destination = publication / "object.bin"
    write_fsync(stage, PAYLOAD)
    fsync_directory(private)
    completed = child(script, root, "publication")
    require_crash(completed, "publication")
    if not stage.exists() or not destination.exists():
        raise CrashCutError("publication link is not visible after process crash")
    stage_stat = stage.stat()
    destination_stat = destination.stat()
    if (stage_stat.st_dev, stage_stat.st_ino) != (destination_stat.st_dev, destination_stat.st_ino):
        raise CrashCutError("published destination is not the staged inode")
    if sha256(stage) != sha256(destination):
        raise CrashCutError("published destination payload differs from staged payload")
    fsync_directory(publication)
    # No-overwrite retry: destination already exists, so a repeated link cannot
    # replace it and must leave the prior destination untouched.
    retry_errno = None
    try:
        os.link(stage, destination)
    except FileExistsError as exc:
        retry_errno = exc.errno
    if retry_errno != errno.EEXIST:
        raise CrashCutError("publication retry did not fail with EEXIST")
    observations = {
        "link_visible_after_crash": True,
        "published_inode_equal": True,
        "published_payload_exact": True,
        "retry_publication_directory_fsync": True,
        "retry_no_overwrite_errno": retry_errno,
    }
    return CaseResult("publication-link-before-dir-sync", True, completed.returncode, observations, round(time.monotonic()-started, 3))


def prepare_retirement_fixture(root: Path) -> tuple[Path, Path]:
    private = root / "private"
    journal = root / "journal"
    stage = private / "restart-stage.bin"
    manifest = journal / "restart-manifest.bin"
    write_fsync(stage, PAYLOAD)
    write_fsync(manifest, b"authenticated-manifest-placeholder\n")
    fsync_directory(private)
    fsync_directory(journal)
    return stage, manifest


def case_retirement_stage_cut(script: Path, root: Path) -> CaseResult:
    started = time.monotonic()
    stage, manifest = prepare_retirement_fixture(root)
    completed = child(script, root, "retirement", "after-stage-unlink")
    require_crash(completed, "retirement stage cut")
    if stage.exists() or not manifest.exists():
        raise CrashCutError("stage-cut observation does not match expected partial cleanup")
    # Restart classification: absent stage + exact manifest remains an explicit
    # partial cleanup state. Finish manifest unlink and sync both directories.
    manifest.unlink()
    fsync_directory(root / "private")
    fsync_directory(root / "journal")
    observations = {
        "stage_absent_after_crash": True,
        "manifest_present_after_crash": True,
        "retry_completed_manifest_unlink": True,
        "retry_directory_fsync": True,
    }
    return CaseResult("retirement-after-stage-unlink", True, completed.returncode, observations, round(time.monotonic()-started, 3))


def case_retirement_both_cut(script: Path, root: Path) -> CaseResult:
    started = time.monotonic()
    stage, manifest = prepare_retirement_fixture(root)
    completed = child(script, root, "retirement", "after-both-unlinks")
    require_crash(completed, "retirement both-unlinks cut")
    if stage.exists() or manifest.exists():
        raise CrashCutError("both-unlinks cut left a cleanup target visible")
    # Restart rule: absent targets can be terminalized only after the relevant
    # directories are synchronized in the retrying process.
    fsync_directory(root / "private")
    fsync_directory(root / "journal")
    observations = {
        "stage_absent_after_crash": True,
        "manifest_absent_after_crash": True,
        "retry_directory_fsync": True,
    }
    return CaseResult("retirement-after-both-unlinks", True, completed.returncode, observations, round(time.monotonic()-started, 3))


def run_harness(scratch: Path) -> dict:
    if sys.platform != "linux":
        raise CrashCutError("process-crash qualification currently requires Linux")
    if not scratch.is_dir():
        raise CrashCutError(f"scratch path is not a directory: {scratch}")
    unique = scratch / f"ucof-phase3-crash-{uuid.uuid4().hex}"
    unique.mkdir(mode=0o700)
    for name in ("private", "publication", "journal"):
        (unique / name).mkdir(mode=0o700)
    fsync_directory(unique)
    script = Path(__file__).resolve()
    results: list[CaseResult] = []
    try:
        results.append(case_checkpoint(script, unique))
        (unique / "private" / "checkpoint.bin").unlink()
        fsync_directory(unique / "private")
        results.append(case_publication(script, unique))
        (unique / "publication" / "object.bin").unlink()
        (unique / "private" / "staged-output.bin").unlink()
        fsync_directory(unique / "publication")
        fsync_directory(unique / "private")
        results.append(case_retirement_stage_cut(script, unique))
        results.append(case_retirement_both_cut(script, unique))
        return {
            "schema": SCHEMA,
            "recorded_utc": datetime.now(timezone.utc).isoformat(),
            "scratch_root": str(scratch.resolve()),
            "ok": all(result.ok for result in results),
            "cases": [asdict(result) for result in results],
            "non_claims": {
                "physical_power_loss_simulated": False,
                "kernel_page_cache_dropped": False,
                "storage_controller_cache_flushed_by_power_cut": False,
                "filesystem_crash_consistency_proven": False,
                "network_filesystem_qualified": False,
                "same_uid_unlink_race_closed": False,
                "production_accepted": False,
            },
        }
    finally:
        shutil.rmtree(unique, ignore_errors=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scratch-dir", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--worker", choices=("checkpoint", "publication", "retirement"))
    parser.add_argument("--root", type=Path)
    parser.add_argument("--cut")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.worker:
        if not args.root:
            print("worker requires --root", file=sys.stderr)
            return 2
        if args.worker == "checkpoint":
            worker_checkpoint(args.root)
        elif args.worker == "publication":
            worker_publication(args.root)
        elif args.worker == "retirement":
            worker_retirement(args.root, args.cut or "")
        return 2

    scratch = args.scratch_dir.resolve() if args.scratch_dir else Path(tempfile.gettempdir()).resolve()
    try:
        report = run_harness(scratch)
    except (OSError, CrashCutError, subprocess.SubprocessError) as exc:
        print(f"Phase 3 process-crash cuts: FAIL: {exc}", file=sys.stderr)
        return 1
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    print(encoded, end="")
    if args.output:
        output = args.output if args.output.is_absolute() else Path.cwd() / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(encoded)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
