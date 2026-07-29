# UCOF-EXP-0001 Experimental Wire Specification

## Status

This document defines a **disposable experimental epoch**. It is not UCOF Core 1.0, carries no compatibility promise, and must not be used for durable storage.

Normative requirement words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are interpreted in their usual standards sense.

## 1. File layout

```text
+----------------------+ offset 0
| File header (32)     |
+----------------------+
| Record 1             |
+----------------------+
| ...                  |
+----------------------+
| Manifest record      |
+----------------------+
| Directory record     |
+----------------------+
| Footer (80)          | exact end of file
+----------------------+
```

There is exactly one directory record and exactly one active manifest selected by the footer.

No padding or trailing bytes are permitted.

## 2. Integer conventions

Unless this document states otherwise, framing integers are unsigned and little-endian.

Readers MUST perform checked arithmetic before adding offsets, header sizes, lengths, or counts.

## 3. File header

The header is exactly 32 bytes.

| Offset | Size | Field | Required value |
|---:|---:|---|---|
| 0 | 8 | magic | `55 43 4f 46 0d 0a 1a 0a` |
| 8 | 4 | epoch | `1` |
| 12 | 4 | flags | `0` |
| 16 | 4 | header length | `32` |
| 20 | 12 | reserved | all zero |

A reader MUST reject a different epoch, non-zero flags, a different header length, or non-zero reserved bytes.

## 4. Record framing

Each record consists of a 40-byte header followed immediately by `stored_len` payload bytes.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | magic `55 43 52 44` (`UCRD`) |
| 4 | 2 | kind |
| 6 | 2 | flags |
| 8 | 4 | record header length |
| 12 | 8 | stored length |
| 20 | 8 | logical length |
| 28 | 8 | object identifier |
| 36 | 4 | reserved |

Requirements:

- flags MUST be zero;
- record header length MUST be 40;
- reserved MUST be zero;
- `stored_len` MUST equal `logical_len` because transforms are absent;
- the complete payload range MUST end no later than the footer;
- records MUST be contiguous and non-overlapping;
- no record may begin inside another record;
- non-directory object identifiers MUST be non-zero and unique;
- the directory record MUST use identifier zero;
- identifier zero MUST NOT be used by opaque or manifest records.

## 5. Experimental record kinds

| Value | Meaning |
|---:|---|
| 1 | Opaque object |
| 2 | Manifest |
| 3 | Primary directory |

A strict `UCOF-EXP-0001` reader MUST reject an unknown record kind. Testing unknown domain semantics is performed through opaque object payloads, not unknown framing kinds.

## 6. Deterministic metadata subset

Manifest and directory payloads use a restricted deterministic CBOR subset.

Permitted values:

- major type 0: unsigned integers;
- major type 2: definite-length byte strings;
- major type 3: definite-length UTF-8 text strings;
- major type 4: definite-length arrays;
- major type 5: definite-length maps;
- simple values false, true, and null.

Rejected values:

- negative integers;
- tags;
- floating-point numbers;
- undefined and unassigned simple values;
- indefinite-length strings, arrays, or maps;
- break markers.

Integer and length arguments MUST use their shortest legal CBOR encoding.

Map entries MUST be ordered by:

1. encoded key byte length, ascending;
2. encoded key bytes, lexicographically ascending.

Encoded map keys MUST be strictly increasing under that ordering. Duplicate keys are therefore invalid.

Text strings MUST contain valid UTF-8. No Unicode normalization is performed by this epoch.

## 7. Manifest schema

The manifest payload is a map with exactly three text keys:

```text
{
  "roots": [uint, ...],
  "required": [uint, ...],
  "optional": [uint, ...]
}
```

Requirements:

- all three keys MUST be present exactly once;
- no additional key is permitted;
- each value MUST be an array of unsigned integers;
- every root identifier MUST be non-zero and name a preceding non-directory record;
- root identifiers MUST be unique;
- capability arrays MUST NOT contain duplicates;
- this epoch defines no supported required capability, so `required` MUST be empty for a conforming file;
- unknown optional identifiers MAY be present and MUST NOT alter structural interpretation.

The manifest record itself MUST use a unique non-zero object identifier.

## 8. Directory schema

The directory payload is:

```text
{
  "entries": [
    {
      "id": uint,
      "kind": uint,
      "offset": uint,
      "stored_len": uint,
      "logical_len": uint
    }, ...
  ]
}
```

Requirements:

- the top-level map MUST contain exactly `entries`;
- each entry map MUST contain exactly the five listed keys;
- entries MUST appear in physical record order;
- one entry MUST exist for every record preceding the directory;
- no entry describes the directory itself;
- every value MUST match the corresponding validated framing header;
- offsets are absolute file offsets to the start of each record header;
- `stored_len` and `logical_len` describe payload bytes, not total record bytes.

A reader MUST validate record framing independently and MUST reject any directory mismatch.

## 9. Footer

The footer is exactly 80 bytes and occupies the last 80 bytes of the file.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | magic `55 43 46 54 52 30 30 31` (`UCFTR001`) |
| 8 | 4 | footer length, `80` |
| 12 | 4 | flags, `0` |
| 16 | 8 | directory record offset |
| 24 | 8 | directory total length |
| 32 | 8 | active manifest object identifier |
| 40 | 8 | total record count including directory |
| 48 | 32 | SHA-256 of committed prefix |

Requirements:

- the directory offset MUST equal the physical offset of the final record;
- directory total length MUST equal `40 + directory stored_len`;
- the directory MUST end exactly where the footer begins;
- the manifest identifier MUST name exactly one preceding manifest record;
- total record count MUST equal the validated count including the directory;
- the digest MUST equal SHA-256 over bytes `[0, footer_offset)`.

The footer is not included in its own digest.

## 10. Validation order

A reader SHOULD validate in this order to provide stable failure categories while avoiding unsafe work:

1. caller file-size limit;
2. minimum length;
3. header magic and fixed fields;
4. footer magic and fixed fields at exact end;
5. checked directory range;
6. sequential record framing and record-count limits;
7. directory position and total length;
8. committed-prefix SHA-256;
9. canonical directory decode and exact framing comparison;
10. manifest selection and canonical decode;
11. capability and root validation.

An implementation MAY calculate the digest concurrently with sequential framing if it produces equivalent results and errors remain categorized.

## 11. Required limits

A reader API MUST permit the caller to bound at least:

- total file bytes;
- number of records;
- single payload bytes;
- metadata payload bytes;
- metadata nesting depth;
- items in one array or map;
- text-string bytes;
- byte-string bytes.

A limit violation MUST be distinguishable from malformed bytes and unsupported capabilities.

## 12. Error categories

The experimental conformance suite uses these conceptual categories:

- `truncated`;
- `invalid_magic`;
- `unsupported_epoch`;
- `unsupported_flags`;
- `invalid_reserved`;
- `invalid_length`;
- `range_out_of_bounds`;
- `duplicate_object_id`;
- `invalid_record_order`;
- `unsupported_record_kind`;
- `non_canonical_metadata`;
- `invalid_metadata_schema`;
- `directory_mismatch`;
- `missing_manifest`;
- `unsupported_required_capability`;
- `digest_mismatch`;
- `limit_exceeded`.

Implementations may expose more specific variants but SHOULD preserve a stable mapping to these categories.

## 13. Writer requirements

A deterministic writer MUST:

- emit the exact fixed header;
- reject zero or duplicate non-structural identifiers;
- emit records contiguously without padding;
- encode manifest and directory metadata canonically;
- construct the directory from actual emitted offsets;
- place the directory last;
- calculate SHA-256 over the committed prefix;
- append one exact footer;
- reject finalization if the selected manifest is missing or has the wrong kind;
- reject writes after finalization.

Given identical ordered inputs and manifest values, output bytes MUST be identical.

## 14. Conformance and retirement

A file conforms to this epoch only if every requirement above passes strict validation.

This epoch may be retired at any time according to the project versioning policy. Later epochs MUST NOT reinterpret these bytes under changed semantics.
