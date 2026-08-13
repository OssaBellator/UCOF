#!/usr/bin/env python3
"""Emit a decision-neutral EXP-0003 candidate-corpus recipe scaffold.

This tool intentionally generates *plans*, not wire bytes or hashes.  It keeps
boundary cases derived from the two explicit D2 geometry alternatives while the
D1-D7 maintainer ballot remains unresolved.

Nothing emitted by this tool is authoritative EXP-0003 material.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
STATUS = "review-only candidate corpus scaffold; no authoritative bytes or hashes"


@dataclass(frozen=True)
class Geometry:
    name: str
    object_id_width: int
    object_header_len: int
    page_header_len: int
    leaf_entry_len: int
    internal_entry_len: int

    @property
    def leaf_capacity(self) -> int:
        return (PAGE_SIZE - self.page_header_len) // self.leaf_entry_len

    @property
    def internal_fanout(self) -> int:
        return (PAGE_SIZE - self.page_header_len) // self.internal_entry_len

    @property
    def leaf_minimum(self) -> int:
        return (self.leaf_capacity + 1) // 2

    @property
    def internal_minimum(self) -> int:
        return (self.internal_fanout + 1) // 2

    @property
    def leaf_overflow_split(self) -> tuple[int, int]:
        count = self.leaf_capacity + 1
        return ((count + 1) // 2, count // 2)

    @property
    def internal_overflow_split(self) -> tuple[int, int]:
        count = self.internal_fanout + 1
        return ((count + 1) // 2, count // 2)

    @property
    def catalog_v2_snapshot_len(self) -> int:
        return 96 + self.object_id_width


GEOMETRIES = (
    Geometry(
        name="tight64-full-range-review-alternative",
        object_id_width=8,
        object_header_len=40,
        page_header_len=40,
        leaf_entry_len=56,
        internal_entry_len=56,
    ),
    Geometry(
        name="tight128-full-range-review-alternative",
        object_id_width=16,
        object_header_len=48,
        page_header_len=56,
        leaf_entry_len=64,
        internal_entry_len=72,
    ),
)


def case(case_id: str, category: str, **fields: object) -> dict[str, object]:
    value: dict[str, object] = {
        "id": case_id,
        "category": category,
        "status": "recipe-only",
    }
    value.update(fields)
    return value


def common_cases() -> list[dict[str, object]]:
    return [
        case("framing.minimal-genesis", "framing", applies_when="always"),
        case("framing.one-linked-append", "framing", applies_when="always"),
        case("framing.strict-exact-end", "framing", applies_when="always"),
        case("framing.trailing-commit-invalid", "framing", applies_when="always"),
        case("framing.torn-publication-invalid", "framing", applies_when="always"),
        case("recovery.previous-valid-prefix", "recovery", applies_when="always"),
        case("identity.object-domain", "identity", applies_when="D6 fixed-domain package selected"),
        case("identity.page-domain", "identity", applies_when="D6 fixed-domain package selected"),
        case("identity.snapshot-domain", "identity", applies_when="D6 fixed-domain package selected"),
        case("identity.commit-domain", "identity", applies_when="D6 fixed-domain package selected"),
        case("identity.cross-domain-separation", "identity", applies_when="D6 fixed-domain package selected"),
        case("framing.magic-corruption-each-structure", "framing", applies_when="D6 fixed-domain package selected"),
        case("framing.nonzero-reserved-invalid", "framing", applies_when="always"),
        case("object.minimum-payload", "object", applies_when="always"),
        case("object.policy-bounded-large-payload", "object", applies_when="always"),
        case("object.digest-mismatch", "object", applies_when="always"),
        case("locator.record-offset-contradiction", "object", applies_when="always"),
        case("locator.record-length-contradiction", "object", applies_when="always"),
        case("object.header-id-mismatch", "object", applies_when="always"),
        case("object.kind-zero-invalid", "object", applies_when="D6 kind package selected"),
        case("object.kind-two-opaque-valid", "object", applies_when="D6 kind package selected"),
        case("object.kind-65535-opaque-valid", "object", applies_when="D6 kind package selected"),
        case("page.unknown-kind-invalid", "page", applies_when="D6 kind package selected"),
        case("mutation.insert-no-split", "mutation", applies_when="always"),
        case("mutation.leaf-split-propagation", "mutation", applies_when="always"),
        case("mutation.internal-split-propagation", "mutation", applies_when="always"),
        case("mutation.delete-no-repair", "mutation", applies_when="always"),
        case("mutation.borrow-left-only", "mutation", applies_when="always"),
        case("mutation.borrow-right-only", "mutation", applies_when="always"),
        case(
            "mutation.two-donor-left-first",
            "mutation",
            applies_when="D4 LeftFirst selected",
            mutually_exclusive_group="D4-two-donor-policy",
        ),
        case(
            "mutation.two-donor-fuller-left-tie",
            "mutation",
            applies_when="D4 FullerSiblingLeftTie selected",
            mutually_exclusive_group="D4-two-donor-policy",
        ),
        case("mutation.merge", "mutation", applies_when="always"),
        case("mutation.recursive-internal-repair", "mutation", applies_when="always"),
        case("mutation.root-collapse", "mutation", applies_when="always"),
        case("mutation.canonical-mixed-batch", "mutation", applies_when="always"),
        case("mutation.caller-order-normalization", "mutation", applies_when="always"),
        case("mutation.unchanged-page-reuse", "mutation", applies_when="always"),
        case("catalog.zero-root-genesis", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.one-root", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.multiple-roots", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.stable-id-unchanged-child", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.stable-id-replacement", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.changed-linked-id-invalid", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.delete-to-catalog-only", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.delete-catalog-slot-invalid", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.known-required-capability", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.unknown-optional-capability", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.unknown-required-capability", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.unknown-extension-preservation", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.missing-root-semantic-invalid", "catalog", applies_when="D5 catalog-v2 selected"),
        case("catalog.malformed-order-length-padding", "catalog", applies_when="D5 catalog-v2 selected"),
        case("determinism.fresh-caller-order-equality", "determinism", applies_when="always"),
        case("determinism.canonical-rewrite-normalization", "determinism", applies_when="always"),
        case(
            "determinism.equal-logical-state-persistent-divergence",
            "determinism",
            applies_when="D7 scoped determinism selected",
        ),
        case(
            "determinism.rewrite-new-byte-identity",
            "determinism",
            applies_when="D7 scoped determinism selected",
        ),
    ]


def geometry_cases(geometry: Geometry) -> list[dict[str, object]]:
    c = geometry.leaf_capacity
    m = geometry.leaf_minimum
    f = geometry.internal_fanout
    fm = geometry.internal_minimum

    values: list[tuple[str, int]] = [
        ("one", 1),
        ("c-minus-1", c - 1),
        ("c", c),
        ("c-plus-1", c + 1),
        ("two-c-minus-1", 2 * c - 1),
        ("two-c", 2 * c),
        ("two-c-plus-1", 2 * c + 1),
        ("c-plus-m-minus-1", c + m - 1),
        ("c-plus-m", c + m),
        ("c-plus-m-plus-1", c + m + 1),
        ("c-times-f-minus-1", c * f - 1),
        ("c-times-f", c * f),
        ("c-times-f-plus-1", c * f + 1),
    ]
    result = [
        case(
            f"occupancy.objects.{name}",
            "occupancy",
            applies_when=f"D2 {geometry.name} selected with D3 half-full policy",
            object_count=value,
        )
        for name, value in values
    ]

    leaf_group_values = (
        ("m-minus-1", m - 1),
        ("m", m),
        ("m-plus-1", m + 1),
        ("c", c),
        ("overflow", c + 1),
    )
    result.extend(
        case(
            f"occupancy.leaf-group.{name}",
            "occupancy",
            applies_when=f"D2 {geometry.name} selected with D3 half-full policy",
            entry_count=value,
        )
        for name, value in leaf_group_values
    )

    internal_group_values = (
        ("m-minus-1", fm - 1),
        ("m", fm),
        ("m-plus-1", fm + 1),
        ("f", f),
        ("overflow", f + 1),
        ("two-f-minus-1", 2 * f - 1),
        ("two-f", 2 * f),
        ("two-f-plus-1", 2 * f + 1),
        ("f-plus-m-minus-1", f + fm - 1),
        ("f-plus-m", f + fm),
        ("f-plus-m-plus-1", f + fm + 1),
    )
    result.extend(
        case(
            f"occupancy.internal-group.{name}",
            "occupancy",
            applies_when=f"D2 {geometry.name} selected with D3 half-full policy",
            child_count=value,
        )
        for name, value in internal_group_values
    )
    return result


def geometry_facts(geometry: Geometry) -> dict[str, object]:
    return {
        **asdict(geometry),
        "page_size": PAGE_SIZE,
        "leaf_capacity": geometry.leaf_capacity,
        "leaf_minimum": geometry.leaf_minimum,
        "leaf_overflow_split": list(geometry.leaf_overflow_split),
        "internal_fanout": geometry.internal_fanout,
        "internal_minimum": geometry.internal_minimum,
        "internal_overflow_split": list(geometry.internal_overflow_split),
        "catalog_v2_snapshot_len_if_D5_selected": geometry.catalog_v2_snapshot_len,
    }


def build_plan(geometry: Geometry) -> dict[str, object]:
    cases = common_cases() + geometry_cases(geometry)
    return {
        "status": STATUS,
        "geometry": geometry_facts(geometry),
        "selection_state": {
            "D1_candidate1": "pending",
            "D2_geometry": "pending",
            "D3_occupancy": "pending",
            "D4_deletion": "pending",
            "D5_catalog": "pending",
            "D6_hash_magic_kind": "pending",
            "D7_determinism": "pending",
        },
        "cases": cases,
    }


def verify(plans: list[dict[str, object]]) -> None:
    by_name = {geometry.name: geometry for geometry in GEOMETRIES}
    tight64 = by_name["tight64-full-range-review-alternative"]
    tight128 = by_name["tight128-full-range-review-alternative"]

    assert (tight64.leaf_capacity, tight64.leaf_minimum) == (291, 146)
    assert (tight64.internal_fanout, tight64.internal_minimum) == (291, 146)
    assert tight64.leaf_overflow_split == (146, 146)
    assert tight64.internal_overflow_split == (146, 146)
    assert tight64.catalog_v2_snapshot_len == 104

    assert (tight128.leaf_capacity, tight128.leaf_minimum) == (255, 128)
    assert (tight128.internal_fanout, tight128.internal_minimum) == (226, 113)
    assert tight128.leaf_overflow_split == (128, 128)
    assert tight128.internal_overflow_split == (114, 113)
    assert tight128.catalog_v2_snapshot_len == 112

    for plan in plans:
        assert plan["status"] == STATUS
        assert all(value == "pending" for value in plan["selection_state"].values())
        cases = plan["cases"]
        ids = [entry["id"] for entry in cases]
        assert len(ids) == len(set(ids)), "duplicate corpus case id"
        assert all(entry["status"] == "recipe-only" for entry in cases)
        serialized = json.dumps(plan, sort_keys=True).lower()
        assert '"authoritative": true' not in serialized
        assert '"accepted": true' not in serialized

        # Both mutually-exclusive D4 recipes must remain present while D4 is pending.
        d4 = [
            entry
            for entry in cases
            if entry.get("mutually_exclusive_group") == "D4-two-donor-policy"
        ]
        assert {entry["id"] for entry in d4} == {
            "mutation.two-donor-left-first",
            "mutation.two-donor-fuller-left-tie",
        }

        # D5/D7 conditional recipes must remain visible rather than silently
        # applying the recommended policy.
        assert any(entry["id"] == "catalog.zero-root-genesis" for entry in cases)
        assert any(
            entry["id"] == "determinism.equal-logical-state-persistent-divergence"
            for entry in cases
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--geometry",
        choices=("all", "tight64", "tight128"),
        default="all",
        help="select one review geometry alternative or emit both",
    )
    parser.add_argument(
        "--verify",
        action="store_true",
        help="assert known derived constants and decision-neutral status",
    )
    args = parser.parse_args()

    selected = list(GEOMETRIES)
    if args.geometry == "tight64":
        selected = [GEOMETRIES[0]]
    elif args.geometry == "tight128":
        selected = [GEOMETRIES[1]]

    plans = [build_plan(geometry) for geometry in selected]
    if args.verify:
        verify(plans)

    print(json.dumps({"plans": plans}, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
