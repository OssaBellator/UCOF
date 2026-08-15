#!/usr/bin/env python3
"""Self-tests for tools/run_phase3_deployment_preflight_bundle.py."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

from tools import run_phase3_deployment_preflight_bundle as bundle

CANDIDATE_SHA = "67beca9ce5d1242c5df839c9ef2d7b1ce5a8b774"
TOOL_SHA = "2" * 40


def successful_checks() -> list[dict]:
    return [
        {
            "name": name,
            "command": ["tool", "--output", f"/tmp/private-child/{index}.json"],
            "status": "pass",
            "returncode": 0,
            "elapsed_seconds": float(index) / 10.0,
            "output": f"private child output {index}",
        }
        for index, name in enumerate(bundle.EXPECTED_INNER_CHECKS, start=1)
    ]


def successful_inner_report() -> dict:
    return {
        "schema": bundle.INNER_SCHEMA,
        "ok": True,
        "checks": successful_checks(),
        "child_evidence_valid": True,
        "child_validation_errors": [],
        "production_policy": "local-mechanical-preflight-only",
        "key_material": {
            "schema": "ucof-phase3-key-material-preflight-v1",
            "ok": True,
            "secret_material_reported": False,
            "keys": [],
        },
        "storage_headroom": {
            "schema": "ucof-phase3-storage-headroom-v1",
            "ok": True,
            "reserved": False,
            "race_free": False,
        },
        "non_claims": {
            "production_accepted": False,
            "power_loss_qualified": False,
            "anti_rollback_qualified": False,
            "same_uid_unlink_race_closed": False,
            "free_space_or_inodes_reserved": False,
            "concurrent_inode_consumption_prevented": False,
            "key_provisioning_or_rotation_qualified": False,
            "ancestor_key_path_pinning_qualified": False,
        },
    }


class DeploymentPreflightBundleTests(unittest.TestCase):
    def test_canonical_git_sha_requires_full_lowercase_hex(self) -> None:
        self.assertEqual(
            bundle.canonical_git_sha(CANDIDATE_SHA, "candidate git SHA"),
            CANDIDATE_SHA,
        )
        for malformed in (
            CANDIDATE_SHA[:-1],
            CANDIDATE_SHA.upper(),
            "g" * 40,
            "phase-3/restart-metadata-compaction",
        ):
            with self.subTest(malformed=malformed), self.assertRaisesRegex(
                bundle.DeploymentBundleError,
                "40 lowercase hexadecimal",
            ):
                bundle.canonical_git_sha(malformed, "candidate git SHA")

    def test_successful_inner_report_preserves_nonclaims_and_check_identity(self) -> None:
        bundle.validate_successful_inner_report(successful_inner_report())

        report = successful_inner_report()
        report["child_evidence_valid"] = False
        with self.assertRaisesRegex(bundle.DeploymentBundleError, "child evidence"):
            bundle.validate_successful_inner_report(report)

        report = successful_inner_report()
        report["checks"][0]["name"] = "unexpected child check"
        with self.assertRaisesRegex(bundle.DeploymentBundleError, "checks changed order or identity"):
            bundle.validate_successful_inner_report(report)

        report = successful_inner_report()
        report["checks"][1]["returncode"] = 1
        with self.assertRaisesRegex(bundle.DeploymentBundleError, "child check is not successful"):
            bundle.validate_successful_inner_report(report)

        report = successful_inner_report()
        report["non_claims"]["production_accepted"] = True
        with self.assertRaisesRegex(bundle.DeploymentBundleError, "non-claim"):
            bundle.validate_successful_inner_report(report)

        report = successful_inner_report()
        report["key_material"]["secret_material_reported"] = True
        with self.assertRaisesRegex(bundle.DeploymentBundleError, "secret-material"):
            bundle.validate_successful_inner_report(report)

        report = successful_inner_report()
        report["storage_headroom"]["reserved"] = True
        with self.assertRaisesRegex(bundle.DeploymentBundleError, "reservation/race"):
            bundle.validate_successful_inner_report(report)

    def test_recursive_path_redaction_prefers_longer_resolved_paths(self) -> None:
        paths = ("/srv/ucof/keys/aes.key", "/srv/ucof")
        redacted = bundle.redact_paths(
            {
                "path": "/srv/ucof/keys/aes.key",
                "command": ["tool", "/srv/ucof/keys/aes.key"],
                "other": "/srv/ucof/data",
            },
            paths,
        )
        rendered = json.dumps(redacted, sort_keys=True)
        self.assertNotIn("/srv/ucof", rendered)
        self.assertIn("<redacted-local-path>", rendered)

    def test_redaction_values_never_include_root_or_current_directory_sentinel(self) -> None:
        values = bundle.redaction_values((Path("/"), Path(".")))
        self.assertNotIn("/", values)
        self.assertNotIn(".", values)

    def test_child_check_summary_omits_commands_and_output_text(self) -> None:
        checks = successful_checks()
        summary = bundle.summarize_inner_checks(checks)
        self.assertEqual([entry["name"] for entry in summary], list(bundle.EXPECTED_INNER_CHECKS))
        self.assertTrue(all(entry["command_persisted"] is False for entry in summary))
        self.assertTrue(all(entry["output_persisted"] is False for entry in summary))
        rendered = json.dumps(summary, sort_keys=True)
        self.assertNotIn("/tmp/private-child", rendered)
        self.assertNotIn("private child output", rendered)
        self.assertTrue(all(entry["output_bytes"] > 0 for entry in summary))

    def test_sanitized_invocation_has_no_local_paths(self) -> None:
        args = argparse.Namespace(
            filesystem_path=Path("/srv/private/data"),
            aes_key=Path("/srv/private/keys/aes.key"),
            hmac_key=Path("/srv/private/keys/hmac.key"),
            required_bytes=12345,
            max_initial_runs=8,
            required_inodes=12,
            reserve_bytes=4096,
            reserve_inodes=3,
        )
        evidence = bundle.sanitized_invocation(args)
        rendered = repr(evidence)
        self.assertNotIn("/srv/private", rendered)
        self.assertTrue(evidence["filesystem_path_supplied"])
        self.assertTrue(evidence["aes_key_path_supplied"])
        self.assertTrue(evidence["hmac_key_path_supplied"])

    def test_main_hashes_raw_report_redacts_paths_and_omits_child_text(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-deployment-bundle-test-") as directory:
            root = Path(directory)
            filesystem = root / "target-filesystem"
            key_parent = root / "private-keys"
            aes = key_parent / "aes.key"
            hmac = key_parent / "hmac.key"
            filesystem.mkdir()
            key_parent.mkdir()
            aes.write_bytes(b"a" * 32)
            hmac.write_bytes(b"b" * 32)
            output = root / "bundle.json"
            fake_preflight = root / "fake_preflight.py"
            private_child_path = "/tmp/ucof-deployment-preflight-private/keys.json"
            fake_preflight.write_text(
                f"""#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
p = argparse.ArgumentParser()
p.add_argument('--filesystem-path', required=True)
p.add_argument('--aes-key', required=True)
p.add_argument('--hmac-key', required=True)
p.add_argument('--required-bytes', required=True)
p.add_argument('--max-initial-runs', required=True)
p.add_argument('--required-inodes', required=True)
p.add_argument('--reserve-bytes', required=True)
p.add_argument('--reserve-inodes', required=True)
p.add_argument('--output', required=True)
a = p.parse_args()
checks = []
for index, name in enumerate({list(bundle.EXPECTED_INNER_CHECKS)!r}, start=1):
    checks.append({{
        'name': name,
        'command': ['tool', '--output', '{private_child_path}', '--aes-key', str(Path(a.aes_key).resolve())],
        'status': 'pass',
        'returncode': 0,
        'elapsed_seconds': index / 10.0,
        'output': 'private temp path {private_child_path} aes=' + str(Path(a.aes_key).resolve()),
    }})
report = {{
  'schema': 'ucof-phase3-deployment-preflight-v3',
  'ok': True,
  'inputs': {{
    'filesystem_path': str(Path(a.filesystem_path).resolve()),
    'aes_key_path': str(Path(a.aes_key).resolve()),
    'hmac_key_path': str(Path(a.hmac_key).resolve()),
  }},
  'checks': checks,
  'child_evidence_valid': True,
  'child_validation_errors': [],
  'filesystem': {{'scratch_root': str(Path(a.filesystem_path).resolve())}},
  'key_material': {{
    'schema': 'ucof-phase3-key-material-preflight-v1',
    'ok': True,
    'secret_material_reported': False,
    'keys': [
      {{'role': 'aes-256', 'path': str(Path(a.aes_key).resolve()), 'parent_path': str(Path(a.aes_key).resolve().parent)}},
      {{'role': 'hmac-sha256', 'path': str(Path(a.hmac_key).resolve()), 'parent_path': str(Path(a.hmac_key).resolve().parent)}},
    ],
  }},
  'storage_headroom': {{
    'schema': 'ucof-phase3-storage-headroom-v1', 'ok': True, 'reserved': False, 'race_free': False,
  }},
  'production_policy': 'local-mechanical-preflight-only',
  'non_claims': {{
    'production_accepted': False,
    'power_loss_qualified': False,
    'anti_rollback_qualified': False,
    'same_uid_unlink_race_closed': False,
    'free_space_or_inodes_reserved': False,
    'concurrent_inode_consumption_prevented': False,
    'key_provisioning_or_rotation_qualified': False,
    'ancestor_key_path_pinning_qualified': False,
  }},
}}
Path(a.output).write_text(json.dumps(report, indent=2) + '\\n')
print('raw filesystem=' + str(Path(a.filesystem_path).resolve()))
print('raw aes=' + str(Path(a.aes_key).resolve()))
"""
            )
            argv = [
                "run_phase3_deployment_preflight_bundle.py",
                "--candidate-git-sha",
                CANDIDATE_SHA,
                "--qualification-tool-git-sha",
                TOOL_SHA,
                "--filesystem-path",
                str(filesystem),
                "--aes-key",
                str(aes),
                "--hmac-key",
                str(hmac),
                "--required-bytes",
                "12345",
                "--max-initial-runs",
                "8",
                "--output",
                str(output),
            ]
            with (
                mock.patch.object(bundle, "PREFLIGHT", fake_preflight),
                mock.patch.object(bundle, "ROOT", root),
                mock.patch.object(sys, "argv", argv),
            ):
                self.assertEqual(bundle.main(), 0)

            report = json.loads(output.read_text())
            self.assertTrue(report["ok"])
            self.assertEqual(report["candidate_git_sha"], CANDIDATE_SHA)
            self.assertEqual(report["qualification_tool_git_sha"], TOOL_SHA)
            self.assertRegex(report["inner_report_sha256"], r"^[0-9a-f]{64}$")
            self.assertFalse(report["preflight"]["raw_output_persisted"])
            rendered = json.dumps(report, sort_keys=True)
            for path in (filesystem.resolve(), key_parent.resolve(), aes.resolve(), hmac.resolve()):
                self.assertNotIn(str(path), rendered)
            self.assertNotIn(private_child_path, rendered)
            self.assertNotIn("private temp path", rendered)
            self.assertIn("<redacted-local-path>", rendered)
            checks = report["deployment_preflight"]["checks"]
            self.assertEqual([check["name"] for check in checks], list(bundle.EXPECTED_INNER_CHECKS))
            self.assertTrue(all(check["command_persisted"] is False for check in checks))
            self.assertTrue(all(check["output_persisted"] is False for check in checks))


if __name__ == "__main__":
    unittest.main()
