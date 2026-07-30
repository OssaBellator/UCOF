#!/usr/bin/env python3
"""Authenticated roots, capabilities, and extension preservation catalog."""

from __future__ import annotations

import struct
from dataclasses import dataclass

import experiment_exp0002_extension_preservation as extensions
import experiment_exp0002_immutable_page_objects as objects

CATALOG_OBJECT_ID = (1 << 64) - 1
CATALOG_KIND = 0xFFFF
CATALOG_MAGIC = b"UCCAT002"
CATALOG_HEADER = struct.Struct("<8sHHIII8s")
CAPABILITY = struct.Struct("<IB3s")
CATALOG_VERSION = 1
CAP_REQUIRED = 1
KNOWN_CAPABILITIES = {1}
KNOWN_EXTENSION_TAG = 1
MAX_ROOTS = 4096
MAX_CAPABILITIES = 4096
MAX_CATALOG_BYTES = 256 * 1024


class CatalogError(ValueError):
    pass


class NotInterpretable(CatalogError):
    pass


@dataclass(frozen=True)
class Capability:
    identifier: int
    required: bool


@dataclass(frozen=True)
class Catalog:
    roots: tuple[int, ...]
    capabilities: tuple[Capability, ...]
    extension_bytes: bytes
    unsupported_required: tuple[int, ...]
    unknown_optional_capabilities: tuple[int, ...]
    parsed_extensions: extensions.ParsedExtensions


@dataclass(frozen=True)
class CatalogReport:
    complete: objects.CompleteReport
    catalog_locator: object
    catalog: Catalog


def encode_catalog(
    roots: list[int], capabilities: list[Capability], extension_bytes: bytes
) -> bytes:
    if not roots or len(roots) > MAX_ROOTS:
        raise CatalogError("root count")
    if roots != sorted(roots) or any(
        root == 0 or left >= right for left, right in zip(roots, roots[1:])
    ):
        raise CatalogError("root order")
    if CATALOG_OBJECT_ID in roots:
        raise CatalogError("catalog cannot be a root")
    if len(capabilities) > MAX_CAPABILITIES:
        raise CatalogError("capability count")
    identifiers = [capability.identifier for capability in capabilities]
    if identifiers != sorted(identifiers) or any(
        identifier == 0
        or left >= right
        for left, right in zip(identifiers, identifiers[1:])
    ):
        raise CatalogError("capability order")
    extensions.parse_extensions(extension_bytes)

    body = bytearray()
    for root in roots:
        body.extend(struct.pack("<Q", root))
    for capability in capabilities:
        body.extend(
            CAPABILITY.pack(
                capability.identifier,
                CAP_REQUIRED if capability.required else 0,
                bytes(3),
            )
        )
    body.extend(extension_bytes)
    total = CATALOG_HEADER.size + len(body)
    if total > MAX_CATALOG_BYTES:
        raise CatalogError("catalog byte limit")
    return CATALOG_HEADER.pack(
        CATALOG_MAGIC,
        CATALOG_VERSION,
        0,
        len(roots),
        len(capabilities),
        len(extension_bytes),
        bytes(8),
    ) + body


def parse_catalog(payload: bytes) -> Catalog:
    if len(payload) < CATALOG_HEADER.size or len(payload) > MAX_CATALOG_BYTES:
        raise CatalogError("catalog byte limit")
    (
        magic,
        version,
        flags,
        root_count,
        capability_count,
        extension_len,
        reserved,
    ) = CATALOG_HEADER.unpack_from(payload)
    if (
        magic != CATALOG_MAGIC
        or version != CATALOG_VERSION
        or flags != 0
        or any(reserved)
        or root_count == 0
        or root_count > MAX_ROOTS
        or capability_count > MAX_CAPABILITIES
    ):
        raise CatalogError("catalog header")

    roots_bytes = root_count * 8
    capabilities_bytes = capability_count * CAPABILITY.size
    expected = (
        CATALOG_HEADER.size
        + roots_bytes
        + capabilities_bytes
        + extension_len
    )
    if expected != len(payload):
        raise CatalogError("catalog length")

    cursor = CATALOG_HEADER.size
    roots = tuple(
        struct.unpack_from("<Q", payload, cursor + index * 8)[0]
        for index in range(root_count)
    )
    cursor += roots_bytes
    if any(
        root == 0 or left >= right for left, right in zip(roots, roots[1:])
    ) or CATALOG_OBJECT_ID in roots:
        raise CatalogError("root order")

    capabilities: list[Capability] = []
    unsupported_required: list[int] = []
    unknown_optional: list[int] = []
    previous = 0
    for _ in range(capability_count):
        identifier, capability_flags, padding = CAPABILITY.unpack_from(
            payload, cursor
        )
        cursor += CAPABILITY.size
        if (
            identifier == 0
            or identifier <= previous
            or capability_flags & ~CAP_REQUIRED
            or any(padding)
        ):
            raise CatalogError("capability record")
        required = bool(capability_flags & CAP_REQUIRED)
        capabilities.append(Capability(identifier, required))
        if identifier not in KNOWN_CAPABILITIES:
            if required:
                unsupported_required.append(identifier)
            else:
                unknown_optional.append(identifier)
        previous = identifier

    extension_bytes = payload[cursor:]
    parsed_extensions = extensions.parse_extensions(extension_bytes)
    return Catalog(
        roots,
        tuple(capabilities),
        extension_bytes,
        tuple(unsupported_required),
        tuple(unknown_optional),
        parsed_extensions,
    )


def validate_catalog(data: bytes) -> CatalogReport:
    complete = objects.validate_complete(data)
    candidates = [
        locator
        for locator in complete.objects
        if locator.object_id == CATALOG_OBJECT_ID
    ]
    if len(candidates) != 1:
        raise CatalogError("catalog object count")
    locator = candidates[0]
    if locator.kind != CATALOG_KIND:
        raise CatalogError("catalog object kind")
    catalog = parse_catalog(complete.object_payloads[CATALOG_OBJECT_ID])
    active_ids = {entry.object_id for entry in complete.objects}
    for root in catalog.roots:
        if root not in active_ids:
            raise CatalogError("missing root object")
    return CatalogReport(complete, locator, catalog)


def require_interpretable(report: CatalogReport) -> None:
    if report.catalog.unsupported_required:
        identifiers = ",".join(
            str(identifier)
            for identifier in report.catalog.unsupported_required
        )
        raise NotInterpretable(
            f"unsupported required capabilities: {identifiers}"
        )


def catalog_input(
    roots: list[int],
    capabilities: list[Capability],
    extension_records: list[extensions.Extension],
) -> objects.ObjectInput:
    payload = encode_catalog(
        roots,
        capabilities,
        extensions.encode_extensions(extension_records),
    )
    return objects.ObjectInput(CATALOG_OBJECT_ID, CATALOG_KIND, payload)


def main() -> None:
    normal = [
        objects.ObjectInput(
            object_id,
            1 + object_id % 3,
            f"payload:{object_id}".encode("ascii"),
        )
        for object_id in range(1, 501)
    ]
    original_extensions = [
        extensions.Extension(KNOWN_EXTENSION_TAG, True, b"known-catalog-v1"),
        extensions.Extension(100, False, b"opaque future metadata"),
        extensions.Extension(200, False, bytes([0, 1, 254, 255])),
    ]
    catalog_value = catalog_input(
        [1, 250, 500],
        [Capability(1, True), Capability(100, False)],
        original_extensions,
    )
    genesis = objects.build_genesis(normal + [catalog_value])
    report = validate_catalog(genesis)
    require_interpretable(report)
    assert report.catalog.roots == (1, 250, 500)
    assert report.catalog.unknown_optional_capabilities == (100,)
    assert [
        record.tag
        for record in report.catalog.parsed_extensions.unknown_optional
    ] == [100, 200]

    # Replacing an unrelated object must reuse the exact catalog record.
    replaced = objects.append_replacement(
        genesis, objects.ObjectInput(1, 9, b"replacement root payload")
    )
    replaced_report = validate_catalog(replaced)
    require_interpretable(replaced_report)
    assert replaced_report.catalog_locator == report.catalog_locator
    assert replaced_report.catalog.extension_bytes == report.catalog.extension_bytes

    # Updating a known extension appends a new catalog object while preserving
    # every unknown optional record byte-for-byte.
    rewritten_extension_bytes = extensions.rewrite_known(
        report.catalog.extension_bytes,
        {KNOWN_EXTENSION_TAG: b"known-catalog-v2-expanded"},
    )
    rewritten_catalog_payload = encode_catalog(
        list(report.catalog.roots),
        list(report.catalog.capabilities),
        rewritten_extension_bytes,
    )
    rewritten = objects.append_replacement(
        genesis,
        objects.ObjectInput(
            CATALOG_OBJECT_ID,
            CATALOG_KIND,
            rewritten_catalog_payload,
        ),
    )
    rewritten_report = validate_catalog(rewritten)
    require_interpretable(rewritten_report)
    assert rewritten_report.catalog_locator.record_offset == len(genesis)
    for tag in (100, 200):
        assert extensions.record_bytes(
            rewritten_report.catalog.extension_bytes, tag
        ) == extensions.record_bytes(report.catalog.extension_bytes, tag)

    # Unknown required capability is visible while structural integrity remains
    # verified; only semantic interpretation fails.
    future_required = objects.append_replacement(
        genesis,
        catalog_input(
            [1, 250, 500],
            [Capability(1, True), Capability(101, True)],
            original_extensions,
        ),
    )
    future_report = validate_catalog(future_required)
    assert future_report.catalog.unsupported_required == (101,)
    try:
        require_interpretable(future_report)
    except NotInterpretable as error:
        interpretability_error = str(error)
    else:
        raise AssertionError("unknown required capability was interpreted")

    # Authenticated malformed metadata must reach catalog validation after outer
    # object, page, snapshot, and commit digests are all valid.
    missing_root = objects.append_replacement(
        genesis,
        catalog_input(
            [1, 250, 999_999],
            [Capability(1, True)],
            original_extensions,
        ),
    )
    try:
        validate_catalog(missing_root)
    except CatalogError as error:
        missing_root_error = str(error)
    else:
        raise AssertionError("missing root object was accepted")
    assert missing_root_error == "missing root object"

    duplicate_root_payload = bytearray(catalog_value.payload)
    root_start = CATALOG_HEADER.size
    duplicate_root_payload[root_start + 8 : root_start + 16] = (
        duplicate_root_payload[root_start : root_start + 8]
    )
    duplicate_root = objects.append_replacement(
        genesis,
        objects.ObjectInput(
            CATALOG_OBJECT_ID,
            CATALOG_KIND,
            bytes(duplicate_root_payload),
        ),
    )
    try:
        validate_catalog(duplicate_root)
    except CatalogError as error:
        duplicate_root_error = str(error)
    else:
        raise AssertionError("duplicate root was accepted")
    assert duplicate_root_error == "root order"

    print(f"catalog_payload_bytes={len(catalog_value.payload)}")
    print(f"roots={report.catalog.roots}")
    print(
        f"capabilities={tuple(capability.identifier for capability in report.catalog.capabilities)}"
    )
    print(
        f"unknown_optional_capabilities={report.catalog.unknown_optional_capabilities}"
    )
    print(
        f"unknown_optional_extension_tags={tuple(record.tag for record in report.catalog.parsed_extensions.unknown_optional)}"
    )
    print(f"unknown_required_interpretability={interpretability_error}")
    print(f"missing_root_rejection={missing_root_error}")
    print(f"duplicate_root_rejection={duplicate_root_error}")
    print("unrelated_rewrite_catalog_reuse=pass")
    print("known_extension_rewrite_preserves_unknown_optional_bytes=pass")
    print("structural_integrity_and_interpretability_are_distinct=pass")
    print("finding=roots and capability declarations can be authenticated as an ordinary immutable object")
    print("finding=unsupported required capabilities prevent interpretation without erasing structural integrity evidence")
    print("finding=unknown optional extension records can survive catalog replacement byte-for-byte")


if __name__ == "__main__":
    main()
