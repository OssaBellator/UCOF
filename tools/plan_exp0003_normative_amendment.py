#!/usr/bin/env python3
"""Plan the coordinated EXP-0003 normative amendment without selecting D1-D7.

This tool does not edit normative files and does not generate wire bytes.  It
maps the maintainer ballot to the exact document/section families and derives
only arithmetic consequences that are already implied by explicit Review
alternatives.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
STATUS = "review-only amendment map; all maintainer decisions pending"


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

SELECTION_STATE = {
    "D1_candidate1": "pending",
    "D2_geometry": "pending",
    "D3_occupancy": "pending",
    "D4_deletion": "pending",
    "D5_catalog": "pending",
    "D6_hash_magic_kind": "pending",
    "D7_determinism": "pending",
}


DOCUMENT_MAP = {
    "docs/proposals/0003-immutable-page-successor.md": [
        "wire-policy summary",
        "identifiers",
        "object header",
        "primary locator",
        "page geometry",
        "occupancy",
        "deterministic insertion",
        "deterministic deletion",
        "catalog/capability policy",
        "scoped determinism",
        "Draft -> Review gates",
    ],
    "spec/experimental/UCOF-EXP-0003.md": [
        "4.2 Object identifiers",
        "4.7 active-tree/catalog-empty semantics",
        "5 cryptographic identity",
        "6 proposed constants",
        "8 object record",
        "9 directory page envelope",
        "10 leaf locator",
        "11 internal child reference",
        "12 snapshot record",
        "13 commit footer snapshot length",
        "15 canonical occupancy",
        "16 persistent insertion",
        "17 persistent deletion",
        "19 canonical bulk vs persistent identity",
        "21 strict validation",
        "22 targeted lookup/absence assurance wording",
        "23 linked history catalog checks if D5 selected",
        "24 recovery catalog checks if D5 selected",
        "25 rewrite/catalog preservation if D5 selected",
        "catalog/capability/extension grammar if D5 selected",
    ],
    "docs/spec/IMMUTABLE_SUCCESSOR_OCCUPANCY_POLICY.md": [
        "all selected capacities/minima",
        "final-two-group redistribution examples",
        "root exceptions",
        "overflow split examples",
        "deletion repair wording",
    ],
    "docs/PHASE_3_DISPOSITION_DRAFT.md": [
        "D1 actual decision record",
        "successor status wording",
    ],
    "docs/PHASE_3_STATUS.md": [
        "selected-policy summary",
        "remaining Review/allocation gates",
    ],
    "docs/EXP_0003_INTEROP_PLAN.md": [
        "P0 selected-policy state",
        "candidate corpus next step",
    ],
    "docs/review/FCP_0002_TO_0003_OBJECTION_TRANSFER.md": [
        "replace stale open-blocker descriptions with dispositions",
    ],
    "docs/review/FCP_0003_DRAFT_TO_REVIEW_LEDGER.md": [
        "record D1-D7 selections without changing historical recommendations",
    ],
}


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
    }


def combination(geometry: Geometry, catalog_selected: bool) -> dict[str, object]:
    snapshot_len = 96 + geometry.object_id_width if catalog_selected else 96
    return {
        "status": "illustrative arithmetic only; not selected",
        "geometry": geometry.name,
        "catalog_v2": "selected-for-combination" if catalog_selected else "omitted-for-combination",
        "wire_arithmetic": {
            "object_id_width": geometry.object_id_width,
            "object_header_len": geometry.object_header_len,
            "page_size": PAGE_SIZE,
            "page_header_len": geometry.page_header_len,
            "leaf_entry_len": geometry.leaf_entry_len,
            "internal_entry_len": geometry.internal_entry_len,
            "leaf_capacity": geometry.leaf_capacity,
            "leaf_minimum": geometry.leaf_minimum,
            "leaf_overflow_split": list(geometry.leaf_overflow_split),
            "internal_fanout": geometry.internal_fanout,
            "internal_minimum": geometry.internal_minimum,
            "internal_overflow_split": list(geometry.internal_overflow_split),
            "snapshot_len": snapshot_len,
            "footer_len": 128,
            "footer_snapshot_length_required": snapshot_len,
        },
        "catalog_consequences": {
            "snapshot_contains_catalog_object_id": catalog_selected,
            "catalog_object_id_width": geometry.object_id_width if catalog_selected else None,
            "catalog_root_id_width": geometry.object_id_width if catalog_selected else None,
            "catalog_only_structural_state_available": catalog_selected,
            "application_root_count_may_be_zero": catalog_selected,
        },
    }


def decision_effects() -> dict[str, object]:
    return {
        "D1": {
            "wire_bytes": "none directly",
            "documents": [
                "docs/PHASE_3_DISPOSITION_DRAFT.md",
                "FCP-0002 status/disposition notice",
                "docs/PHASE_3_STATUS.md",
                "issues #13 and #76",
            ],
        },
        "D2": {
            "wire_bytes": "ObjectId width, object/page headers, leaf/internal entry widths",
            "derives": "D3 numeric capacity/minimum/split values and D5 catalog/snapshot widths",
        },
        "D3": {
            "wire_bytes": "tree shape and deterministic grouping/split identities",
            "depends_on": "D2 selected geometry",
        },
        "D4": {
            "wire_bytes": "policy-significant persistent deletion output when both siblings can lend",
            "does_not_change": "fixed field widths",
        },
        "D5": {
            "wire_bytes": "snapshot catalog field/length plus catalog payload grammar",
            "depends_on": "D2 ObjectId width",
            "cross_constraint": "if catalog is selected, D6/revised kind policy must assign one Core-recognized catalog kind",
        },
        "D6": {
            "wire_bytes": "hash domains, structural magics, page/object kind validity",
            "does_not_change": "digest field width under recommended package",
        },
        "D7": {
            "wire_bytes": "allowed persistent transition identity/tree-history outcomes",
            "does_not_change": "fixed field widths under recommended scoped-determinism option",
            "corpus_consequence": "equal logical state may intentionally have different persistent root/snapshot identities",
        },
    }


def build_plan() -> dict[str, object]:
    return {
        "status": STATUS,
        "selection_state": dict(SELECTION_STATE),
        "decision_effects": decision_effects(),
        "document_map": DOCUMENT_MAP,
        "geometry_alternatives": [geometry_facts(g) for g in GEOMETRIES],
        "D2_x_D5_arithmetic": [
            combination(geometry, catalog_selected)
            for geometry in GEOMETRIES
            for catalog_selected in (False, True)
        ],
        "post_ballot_order": [
            "record explicit D1-D7 maintainer dispositions",
            "apply one coordinated normative amendment",
            "run consistency checks over selected lengths/capacities/algorithms",
            "promote applicable #119 recipe IDs to concrete candidate byte generators",
            "generate and reproduce candidate valid/invalid corpus",
            "move FCP-0003 Draft -> Review only when spec and candidate corpus agree",
            "require clean-room interpretation/reproduction before explicit experimental allocation",
        ],
    }


def verify(plan: dict[str, object]) -> None:
    assert plan["status"] == STATUS
    assert all(value == "pending" for value in plan["selection_state"].values())

    by_name = {geometry.name: geometry for geometry in GEOMETRIES}
    tight64 = by_name["tight64-full-range-review-alternative"]
    tight128 = by_name["tight128-full-range-review-alternative"]

    assert (tight64.leaf_capacity, tight64.leaf_minimum) == (291, 146)
    assert (tight64.internal_fanout, tight64.internal_minimum) == (291, 146)
    assert tight64.leaf_overflow_split == (146, 146)
    assert tight64.internal_overflow_split == (146, 146)

    assert (tight128.leaf_capacity, tight128.leaf_minimum) == (255, 128)
    assert (tight128.internal_fanout, tight128.internal_minimum) == (226, 113)
    assert tight128.leaf_overflow_split == (128, 128)
    assert tight128.internal_overflow_split == (114, 113)

    combos = {
        (entry["geometry"], entry["catalog_v2"]): entry
        for entry in plan["D2_x_D5_arithmetic"]
    }
    assert combos[(tight64.name, "omitted-for-combination")]["wire_arithmetic"]["snapshot_len"] == 96
    assert combos[(tight64.name, "selected-for-combination")]["wire_arithmetic"]["snapshot_len"] == 104
    assert combos[(tight128.name, "omitted-for-combination")]["wire_arithmetic"]["snapshot_len"] == 96
    assert combos[(tight128.name, "selected-for-combination")]["wire_arithmetic"]["snapshot_len"] == 112

    serialized = json.dumps(plan, sort_keys=True).lower()
    assert '"accepted": true' not in serialized
    assert '"authoritative": true' not in serialized
    assert '"allocated": true' not in serialized
    assert "move fcp-0003 draft -> review only when spec and candidate corpus agree" in serialized


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()

    plan = build_plan()
    if args.verify:
        verify(plan)
    print(json.dumps(plan, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
