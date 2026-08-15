#!/usr/bin/env python3
"""Build a deterministic Phase 3 clean-room handoff bundle.

The bundle is intentionally restricted to maintainer-supplied normative text
and public vector/corpus inputs. Implementation source, build metadata, review
notes and experiment internals are rejected by policy. By default all D1-D7
decisions must be explicitly selected before a final bundle can be built.

This tool does not make the bundled text normative; it packages an already
reviewed selection for independent interpretation/reproduction.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
from tools import verify_phase3_decision_state as decisions

FORBIDDEN_TOP_LEVEL = {"crates", "tools", "fuzz", ".github", "target"}
FORBIDDEN_SUFFIXES = {
    ".rs",
    ".py",
    ".pyc",
    ".sh",
    ".ps1",
    ".toml",
    ".lock",
    ".yml",
    ".yaml",
}
FORBIDDEN_DOC_COMPONENTS = {
    "experiments",
    "review",
    "verification",
}
MANIFEST_NAME = "CLEANROOM_MANIFEST.json"
FIXED_ZIP_DATE = (1980, 1, 1, 0, 0, 0)


class CleanroomError(RuntimeError):
    pass


@dataclass(frozen=True)
class InputFile:
    path: str
    bytes: int
    sha256: str


def repo_relative(path: Path) -> Path:
    resolved = path.resolve()
    try:
        relative = resolved.relative_to(ROOT.resolve())
    except ValueError as exc:
        raise CleanroomError(f"input is outside repository: {path}") from exc
    if any(part == ".." for part in relative.parts):
        raise CleanroomError(f"input escapes repository: {path}")
    return relative


def reject_implementation_path(relative: Path) -> None:
    if not relative.parts:
        raise CleanroomError("repository root cannot be a clean-room input")
    if relative.parts[0] in FORBIDDEN_TOP_LEVEL:
        raise CleanroomError(f"implementation/build path is forbidden: {relative}")
    if relative.suffix.lower() in FORBIDDEN_SUFFIXES:
        raise CleanroomError(f"implementation/build file type is forbidden: {relative}")
    lowered = {part.lower() for part in relative.parts}
    if relative.parts[0] == "docs" and lowered.intersection(FORBIDDEN_DOC_COMPONENTS):
        raise CleanroomError(f"internal experiment/review documentation is forbidden: {relative}")


def collect_path(path: Path) -> list[Path]:
    relative = repo_relative(path)
    if not path.exists():
        raise CleanroomError(f"clean-room input does not exist: {relative}")
    if path.is_symlink():
        raise CleanroomError(f"symlink clean-room input is forbidden: {relative}")
    if path.is_file():
        reject_implementation_path(relative)
        return [path]
    if not path.is_dir():
        raise CleanroomError(f"unsupported clean-room input type: {relative}")
    files: list[Path] = []
    for child in sorted(path.rglob("*"), key=lambda item: item.as_posix()):
        if child.is_symlink():
            raise CleanroomError(
                f"symlink inside clean-room input is forbidden: {repo_relative(child)}"
            )
        if child.is_file():
            reject_implementation_path(repo_relative(child))
            files.append(child)
    if not files:
        raise CleanroomError(f"clean-room input directory is empty: {relative}")
    return files


def collect_inputs(paths: list[Path]) -> list[Path]:
    if not paths:
        raise CleanroomError("at least one --input path is required")
    unique: dict[str, Path] = {}
    for path in paths:
        absolute = path if path.is_absolute() else ROOT / path
        for file_path in collect_path(absolute):
            relative = repo_relative(file_path).as_posix()
            unique.setdefault(relative, file_path)
    return [unique[name] for name in sorted(unique)]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def git_sha() -> str | None:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return completed.stdout.strip() if completed.returncode == 0 else None


def decision_summary(require_all_selected: bool) -> dict:
    payload = decisions.load(decisions.DEFAULT_STATE)
    summary = decisions.validate(payload)
    if require_all_selected and not summary["all_selected"]:
        raise CleanroomError(
            "D1-D7 are not all selected; final clean-room handoff is intentionally blocked"
        )
    return summary


def build_manifest(files: list[Path], require_all_selected: bool) -> dict:
    decision_state = decision_summary(require_all_selected)
    records = [
        InputFile(
            path=repo_relative(path).as_posix(),
            bytes=path.stat().st_size,
            sha256=sha256_file(path),
        )
        for path in files
    ]
    return {
        "schema": "ucof-phase3-cleanroom-handoff-v1",
        "git_sha": git_sha(),
        "normative_effect": False,
        "decision_state": decision_state,
        "implementation_source_included": False,
        "files": [record.__dict__ for record in records],
        "instructions": [
            "Interpret the bundled normative/public corpus material without consulting UCOF implementation source.",
            "Record every ambiguity or required private clarification as a specification defect.",
            "Reproduce the public candidate corpus from an independently maintained implementation.",
            "Do not treat matching bytes alone as evidence that unstated rules are specified.",
        ],
    }


def manifest_bytes(manifest: dict) -> bytes:
    return (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_zip(output: Path, files: list[Path], manifest: dict) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in files:
            relative = repo_relative(path).as_posix()
            info = zipfile.ZipInfo(relative, FIXED_ZIP_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, path.read_bytes())
        info = zipfile.ZipInfo(MANIFEST_NAME, FIXED_ZIP_DATE)
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o100644 << 16
        archive.writestr(info, manifest_bytes(manifest))


def verify_zip(path: Path, expected_manifest: dict) -> None:
    with zipfile.ZipFile(path, "r") as archive:
        names = archive.namelist()
        expected_names = [record["path"] for record in expected_manifest["files"]] + [MANIFEST_NAME]
        if names != expected_names:
            raise CleanroomError("clean-room archive member order/content mismatch")
        archived_manifest = json.loads(archive.read(MANIFEST_NAME))
        if archived_manifest != expected_manifest:
            raise CleanroomError("clean-room archived manifest mismatch")
        for record in expected_manifest["files"]:
            payload = archive.read(record["path"])
            if len(payload) != record["bytes"]:
                raise CleanroomError(f"clean-room archived length mismatch: {record['path']}")
            if hashlib.sha256(payload).hexdigest() != record["sha256"]:
                raise CleanroomError(f"clean-room archived digest mismatch: {record['path']}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, action="append", default=[])
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--allow-unselected-plan",
        action="store_true",
        help="permit manifest/plan generation while D1-D7 remain unselected; cannot be represented as final readiness",
    )
    parser.add_argument(
        "--manifest-only",
        action="store_true",
        help="print the deterministic manifest without writing a ZIP",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        files = collect_inputs(args.input)
        require_all_selected = not args.allow_unselected_plan
        manifest = build_manifest(files, require_all_selected)
        if args.manifest_only:
            print(manifest_bytes(manifest).decode("utf-8"), end="")
            return 0
        if not args.output:
            raise CleanroomError("--output is required unless --manifest-only is used")
        output = args.output if args.output.is_absolute() else ROOT / args.output
        write_zip(output, files, manifest)
        verify_zip(output, manifest)
    except (OSError, CleanroomError, zipfile.BadZipFile, json.JSONDecodeError) as exc:
        print(f"Phase 3 clean-room handoff: FAIL: {exc}", file=sys.stderr)
        return 1
    print("Phase 3 clean-room handoff: PASS")
    print(f"output={output}")
    print(f"files={len(files)}")
    print(f"decisions_all_selected={str(manifest['decision_state']['all_selected']).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
