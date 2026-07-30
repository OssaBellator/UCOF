# Experiment 0020: Canonical Extension Preservation

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** FCP-0002 future-field and capability preservation
- **Script:** `tools/experiment_exp0002_extension_preservation.py`

## Question

How can a future UCOF snapshot preserve unknown optional metadata without interpreting it, while unknown required metadata fails closed?

## Candidate 1 limitation

Candidate 1 requires reserved bytes to be zero. This is useful for canonicality and covert-byte elimination, but reserved-zero regions cannot carry or preserve unknown future data.

Capability arrays declare interpretation requirements, but they do not identify or delimit arbitrary unknown fields inside fixed structures.

## Prototype extension block

The experiment defines a separate canonical length-delimited block:

- fixed block magic;
- exact record count and total byte length;
- records sorted by unique non-zero `u16` tag;
- one required/critical flag;
- explicit payload length;
- zero padding to eight-byte alignment;
- caller limits for total bytes and record count.

Known tags are interpreted. Unknown optional tags are retained as opaque payloads. Unknown required tags stop parsing with an unsupported-required result.

## Preservation rule

A rewrite that changes a known record must preserve every unknown optional record unless an explicit higher-level policy authorizes dropping it.

Because the parser enforces canonical order, flags, lengths, and zero padding, re-encoding an unknown optional record reproduces its exact record bytes. The executable test changes one known required record and requires two unknown optional records to remain byte-identical.

## Rejections tested

The prototype rejects:

- unknown required records;
- duplicate or unordered tags;
- tag zero;
- unknown flag bits;
- non-zero alignment padding;
- truncated records or payloads;
- trailing bytes;
- record-count and byte limits;
- attempts to rewrite an unknown tag as though it were known.

## Findings

1. Reserved-zero bytes are not an extensibility or preservation mechanism.
2. Unknown optional data needs explicit length-delimited ownership.
3. Required criticality belongs on each extension record or on an unambiguous enclosing capability contract.
4. Canonical ordering and zero padding allow deterministic opaque preservation.
5. Rewrite, repair, and compaction tools need an explicit preservation policy; silent dropping is unsafe.
6. Unknown optional preservation does not imply semantic understanding or permission to execute embedded data.

## Open questions

A successor proposal still must decide:

- which structures may carry extension blocks;
- whether tags are global, structure-local, profile-local, or capability-scoped;
- whether unknown optional records are included in structural snapshot identity;
- whether compaction may drop unreferenced optional extension objects;
- how extension payload confidentiality and signatures interact with canonical bytes;
- permanent registry and collision-avoidance rules.

## Reproduction

```console
python3 tools/experiment_exp0002_extension_preservation.py
```
