# Experiment 0001 — Fixed-width versus variable-width record framing

**Status:** Reproducible Phase 1 evidence  
**Script:** `tools/experiment_framing_widths.py`  
**Scope:** `UCOF-EXP-0001` only

## Question

Does the experiment's 40-byte fixed record header impose enough storage overhead to justify variable-width framing before the next wire epoch?

## Compared layouts

The current experiment uses a 40-byte record header with fixed-width little-endian fields.

The comparison candidate is intentionally compact and incomplete: an 8-byte fixed prefix followed by unsigned LEB128 encodings of stored length, logical length, and object identifier. It omits the current explicit header-length field and therefore gives up part of the fixed layout's extension and direct-offset behavior.

This candidate is a measurement device, not a proposed format.

## Results

| Workload | Records | Fixed header bytes | Compact candidate bytes | Header saving | Whole-file saving |
|---|---:|---:|---:|---:|---:|
| minimal | 2 | 80 | 24 | 70.0% | 20.5882% |
| small-archive | 1,002 | 40,080 | 13,901 | 65.3% | 7.2690% |
| table-pages | 10,002 | 400,080 | 159,903 | 60.0% | 0.0366% |
| large-media | 1,002 | 40,080 | 17,903 | 55.3% | 0.0000% |

## Interpretation

Variable-width framing materially reduces header bytes for many small records. The benefit becomes negligible as a percentage of the complete file when payloads are page-sized or media-sized.

The fixed layout retains important experimental advantages:

- constant field offsets;
- bounded parsing without a varint loop;
- simple mutation and differential tests;
- direct validation of a declared header length;
- easier inspection in a hex dump.

The compact candidate introduces additional concerns:

- non-shortest and overflowing varints require canonicality rules;
- field offsets depend on preceding values;
- malformed inputs can increase parser work;
- extension space must be redesigned because the explicit header length was removed.

## Decision for EXP-0001

Retain fixed-width framing for this disposable epoch. Do not infer that 40 bytes is suitable for a stable version.

A later experiment should evaluate a hybrid layout: a small fixed bootstrap plus a bounded canonical variable-length extension area. That comparison must include parser complexity, range-read behavior, malformed-varint tests, and profile workloads dominated by tiny records.
