#!/usr/bin/env python3
"""Verify provider-neutral conditional HTTP response classification traces."""

from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = (
    ROOT
    / "tests"
    / "vectors"
    / "exp-0002-immutable-transport"
    / "http-classification-traces.json"
)

RETRYABLE = {
    408: "http-request-timeout",
    425: "http-too-early",
    429: "http-rate-limited",
    500: "http-internal-server-error",
    502: "http-bad-gateway",
    503: "http-service-unavailable",
    504: "http-gateway-timeout",
}


def strong_version(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) >= 2
        and not value.startswith("W/")
        and value.startswith('"')
        and value.endswith('"')
        and '"' not in value[1:-1]
    )


def failure(error: str) -> dict[str, Any]:
    return {"decision": "fail", "error": error}


def classify(case: dict[str, Any]) -> dict[str, Any]:
    request = case["request"]
    response = case["response"]
    status = response["status"]
    if status in RETRYABLE:
        minimum = response.get("retry_after_millis")
        if minimum == 0:
            minimum = None
        result: dict[str, Any] = {
            "decision": "retry",
            "error": f"retryable:{RETRYABLE[status]}",
            "server_minimum_millis": minimum,
        }
        if "backoff" in case:
            backoff = case["backoff"]
            base = backoff["base"]
            maximum = backoff["max"]
            cumulative = backoff["cumulative"]
            if base <= 0 or maximum < base or cumulative < base:
                result["delay_error"] = "retry-delay-configuration"
            elif minimum is not None and minimum > maximum:
                result["delay_error"] = "server-retry-delay"
            else:
                delay = max(base, minimum or 0)
                remaining = backoff.get("remaining_deadline")
                if remaining is not None and delay >= remaining:
                    result["delay_error"] = "deadline"
                elif delay > cumulative:
                    result["delay_error"] = "retry-delay"
                else:
                    result["delay_millis"] = delay
        return result

    authentication = case["authentication"]
    if status == 401:
        if authentication == "one-refresh":
            return {"decision": "refresh-authentication"}
        return failure("client:http-unauthorized")
    if status == 403:
        return failure("client:http-forbidden")
    if status == 404:
        return failure("client:http-object-not-found")
    if status == 412:
        return failure("version-changed")
    if status == 416:
        return failure("client:http-range-unsatisfiable")
    if status in {301, 302, 303, 307, 308}:
        return failure("protocol:redirect")

    kind = request["kind"]
    if kind == "metadata" and status == 200:
        if "content_range" in response or response["body_length"] != 0:
            return failure("protocol:metadata-response-shape")
        if "content_length" not in response:
            return failure("protocol:metadata-length")
        version = response.get("version")
        if not strong_version(version):
            return failure("invalid-version-token")
        return {
            "decision": "accept-metadata",
            "length": response["content_length"],
            "version": version,
        }

    if kind == "range" and status == 206:
        offset = request["offset"]
        length = request["length"]
        total_length = request["total_length"]
        if length <= 0:
            return failure("protocol:zero-range")
        if offset + length > total_length:
            return failure("protocol:range-outside-object")
        if response.get("content_length") != length or response["body_length"] != length:
            return failure("protocol:partial-response-length")
        if response.get("content_range") != [offset, offset + length - 1, total_length]:
            return failure("protocol:content-range")
        version = response.get("version")
        if not strong_version(version):
            return failure("invalid-version-token")
        if version != request["version"]:
            return failure("protocol:response-version-token")
        return {
            "decision": "accept-range",
            "version": version,
            "offset": offset,
            "total_length": total_length,
            "body_length": length,
        }

    if 200 <= status < 300:
        return failure(
            "protocol:metadata-success-status"
            if kind == "metadata"
            else "protocol:range-success-status"
        )
    if 400 <= status < 500:
        return failure("client:http-client-status")
    if 500 <= status < 600:
        return failure("client:http-server-status")
    return failure("protocol:unexpected-http-status")


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative conditional HTTP response classification traces":
        raise AssertionError("classification trace status")
    cases = contract["cases"]
    names = [case["name"] for case in cases]
    if len(cases) != 17 or len(names) != len(set(names)):
        raise AssertionError("classification trace set")

    aggregate = sha256()
    for case in cases:
        actual = classify(case)
        if actual != case["expected"]:
            raise AssertionError(
                f"{case['name']}: expected {case['expected']!r}, received {actual!r}"
            )
        aggregate.update(case["name"].encode("utf-8"))
        aggregate.update(
            json.dumps(actual, sort_keys=True, separators=(",", ":")).encode("utf-8")
        )
        print(f"{case['name']}: {json.dumps(actual, sort_keys=True)}")

    actual_aggregate = aggregate.hexdigest()
    if actual_aggregate != contract["aggregate_sha256"]:
        raise AssertionError(
            f"aggregate expected {contract['aggregate_sha256']}, received {actual_aggregate}"
        )
    print(f"classification_traces={len(cases)}")
    print(f"aggregate_sha256={actual_aggregate}")
    print("explicit_retry_allowlist=pass")
    print("terminal_version_and_protocol_errors=pass")
    print("authorization_refresh_is_explicit=pass")
    print("bounded_retry_after_composition=pass")


if __name__ == "__main__":
    main()
