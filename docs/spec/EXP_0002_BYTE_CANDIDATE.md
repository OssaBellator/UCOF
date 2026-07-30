# UCOF-EXP-0002 Provisional Byte Candidate

**Status:** Experimental draft; disposable and non-stable  
**Experimental epoch:** `UCOF-EXP-0002`  
**Decision basis:** ADR-0010 and FCP-0002  
**Byte order:** little-endian  
**Digest:** SHA-256, algorithm identifier `1`

This document defines the first independently implementable byte candidate for Phase 3 experiments. It is not a compatibility promise. Implementations must reject unknown or malformed values rather than infer intent, and files produced during this experiment may be retired without migration support.

## 1. Scope

The candidate defines:

- a fixed bootstrap header;
- opaque object records;
- fixed-size authenticated directory pages;
- snapshot records;
- exact-end commit footers;
- deterministic digest preimages;
- strict validation;
- explicit previous-footer recovery traversal.

It does not define transforms, compression, encryption, signatures, schemas, external references, mutable in-place pages, progress checkpoints, or permanent registry values.

## 2. Common rules

1. All unsigned integers are little-endian.
2. Every offset is absolute from byte zero of the file.
3. Every length includes the complete referenced structure unless stated otherwise.
4. Arithmetic must be checked before range construction or host-size conversion.
5. Reserved bytes and unused page bytes must be zero.
6. Unknown non-zero flags are invalid in this candidate.
7. SHA-256 digests are exactly 32 bytes.
8. `0xffff_ffff_ffff_ffff` is the absent-offset sentinel.
9. Strict validation reads only the footer ending at the exact file end. It never falls back to scanning.
10. Recovery is a separately requested operation with independent byte, candidate, validation, chain-depth, and result limits.

## 3. Digest domains

The ASCII domain prefixes include the trailing NUL byte:

| Purpose | Domain bytes |
|---|---|
| Object record | `UCOF-EXP-0002-OBJECT\0` |
| Directory page | `UCOF-EXP-0002-PAGE\0` |
| Snapshot | `UCOF-EXP-0002-SNAPSHOT\0` |
| Commit | `UCOF-EXP-0002-COMMIT\0` |

No digest preimage is shared between structure classes.

## 4. File header

The file begins with one 64-byte header.

| Offset | Size | Field | Required value |
|---:|---:|---|---|
| 0 | 8 | magic | `55 43 4f 46 32 0d 0a 1a` (`UCOF2`, CR, LF, SUB) |
| 8 | 2 | experimental version | `2` |
| 10 | 2 | header length | `64` |
| 12 | 4 | flags | `0` |
| 16 | 4 | directory page size | `16384` |
| 20 | 2 | digest algorithm | `1` |
| 22 | 2 | reserved | zero |
| 24 | 16 | file identifier | caller-selected opaque bytes |
| 40 | 16 | creation nonce | caller-selected opaque bytes |
| 56 | 8 | reserved | zero |

The file identifier and creation nonce are not interpreted by the core experiment. Deterministic vectors use published fixed values.

## 5. Object record

An object record is a 48-byte header followed immediately by `payload_length` opaque bytes.

### 5.1 Object header

| Offset | Size | Field | Constraint |
|---:|---:|---|---|
| 0 | 4 | magic | `OBJ2` |
| 4 | 2 | header length | `48` |
| 6 | 2 | object kind | experimental non-zero value |
| 8 | 4 | flags | `0` |
| 12 | 8 | object identifier | non-zero and unique in one snapshot directory |
| 20 | 8 | payload length | bounded by reader policy |
| 28 | 8 | logical length | equals payload length in this candidate |
| 36 | 12 | reserved | zero |

`record_length = 48 + payload_length` using checked arithmetic.

### 5.2 Object digest

`object_digest = SHA256(OBJECT_DOMAIN || exact_object_header || exact_payload)`.

The digest is stored in the referencing leaf entry, not in the object record. A leaf entry therefore authenticates both the physical locator and the record bytes.

## 6. Directory page

Every directory page is exactly 16,384 bytes. Page bytes consist of a 64-byte common header, packed fixed-size entries, then zero padding.

### 6.1 Common page header

| Offset | Size | Field | Constraint |
|---:|---:|---|---|
| 0 | 4 | magic | `PG02` |
| 4 | 1 | page kind | `1` leaf, `2` internal |
| 5 | 1 | level | `0` leaf; internal greater than zero |
| 6 | 2 | header length | `64` |
| 8 | 2 | entry count | within capacity and non-zero |
| 10 | 2 | entry size | `88` leaf, `64` internal |
| 12 | 4 | flags | `0` |
| 16 | 8 | minimum key | first entry minimum identifier |
| 24 | 8 | maximum key | last entry maximum identifier |
| 32 | 8 | page sequence | snapshot sequence that created this page |
| 40 | 24 | reserved | zero |

Page ranges are inclusive. Keys and child ranges must be strictly ordered and non-overlapping.

### 6.2 Leaf entry

A leaf page can contain at most 185 entries.

| Entry offset | Size | Field | Constraint |
|---:|---:|---|---|
| 0 | 8 | object identifier | strictly increasing, non-zero |
| 8 | 2 | object kind | agrees with object header |
| 10 | 2 | flags | `0` |
| 12 | 4 | reserved | zero |
| 16 | 8 | record offset | exact object-record start |
| 24 | 8 | record length | exact header plus payload length |
| 32 | 8 | logical length | agrees with object header |
| 40 | 32 | object digest | digest defined in section 5.2 |
| 72 | 16 | reserved | zero |

The referenced range must be in bounds and must not overlap the page, snapshot, footer, or another referenced object record in the selected snapshot.

### 6.3 Internal entry

An internal page can contain at most 255 entries.

| Entry offset | Size | Field | Constraint |
|---:|---:|---|---|
| 0 | 8 | child minimum key | inclusive |
| 8 | 8 | child maximum key | inclusive and not less than minimum |
| 16 | 8 | child page offset | exact page start |
| 24 | 4 | child page length | `16384` |
| 28 | 2 | child level | exactly parent level minus one |
| 30 | 2 | flags | `0` |
| 32 | 32 | child page digest | digest defined in section 6.4 |

The referenced child header range must exactly equal the entry range. Repeated page offsets, cycles, inconsistent levels, invalid page lengths, and forged ranges are invalid.

### 6.4 Page digest

`page_digest = SHA256(PAGE_DOMAIN || exact_16384_page_bytes)`.

Unused bytes after the final entry are part of the digest and must be zero.

## 7. Snapshot record

A snapshot is a 160-byte header followed by three packed `u64` arrays in this order:

1. root object identifiers;
2. required capability identifiers;
3. optional capability identifiers.

The exact snapshot length is:

`160 + 8 * (root_count + required_count + optional_count)`

with checked arithmetic.

### 7.1 Snapshot header

| Offset | Size | Field | Constraint |
|---:|---:|---|---|
| 0 | 4 | magic | `SNP2` |
| 4 | 2 | header length | `160` |
| 6 | 2 | flags | `0`; complete snapshots only |
| 8 | 8 | sequence | genesis `0`; child equals parent plus one |
| 16 | 32 | parent snapshot digest | zero for genesis, otherwise exact parent snapshot digest |
| 48 | 8 | previous footer offset | sentinel for genesis, otherwise exact previous footer start |
| 56 | 8 | directory root offset | exact root-page start |
| 64 | 4 | directory root length | `16384` |
| 68 | 2 | directory root level | agrees with root page |
| 70 | 2 | digest algorithm | `1` |
| 72 | 32 | directory root digest | digest defined in section 6.4 |
| 104 | 4 | root count | bounded by policy |
| 108 | 4 | required capability count | bounded by policy |
| 112 | 4 | optional capability count | bounded by policy |
| 116 | 44 | reserved | zero |

Arrays contain canonical ascending unique `u64` values. Root identifiers must exist in the selected directory. Required and optional capability sets must not overlap. This first candidate defines no non-zero capability value, so conforming experimental writers emit empty capability arrays.

### 7.2 Snapshot digest

`snapshot_digest = SHA256(SNAPSHOT_DOMAIN || exact_snapshot_record_bytes)`.

## 8. Commit footer

A commit becomes published only when a complete 160-byte footer is appended. The active strict footer starts at `file_length - 160`.

| Offset | Size | Field | Constraint |
|---:|---:|---|---|
| 0 | 8 | magic | `UCOF2END` |
| 8 | 2 | footer length | `160` |
| 10 | 2 | experimental version | `2` |
| 12 | 4 | flags | `0` |
| 16 | 8 | commit start | `0` for genesis; previous footer end for append |
| 24 | 8 | commit length before footer | current footer offset minus commit start |
| 32 | 8 | snapshot offset | exact snapshot start |
| 40 | 8 | snapshot length | exact snapshot record length |
| 48 | 8 | sequence | agrees with snapshot |
| 56 | 8 | previous footer offset | sentinel for genesis, otherwise earlier than current footer |
| 64 | 8 | object record count | number of object records physically written in this commit |
| 72 | 2 | digest algorithm | `1` |
| 74 | 6 | reserved | zero |
| 80 | 32 | snapshot digest | digest defined in section 7.2 |
| 112 | 32 | commit digest | digest defined in section 8.1 |
| 144 | 16 | reserved | zero |

The snapshot previous-footer field and footer previous-footer field must agree.

### 8.1 Commit digest

Let `footer_semantics` be footer bytes `8..112`, which include the footer length, version, flags, commit range, snapshot locator, sequence, previous-footer locator, record count, algorithm identifier, zero reserved bytes, and snapshot digest, but exclude footer magic, commit digest, and trailing reserved bytes.

`commit_digest = SHA256(COMMIT_DOMAIN || exact_file_bytes[commit_start..footer_offset] || footer_semantics)`.

For genesis, the committed byte range includes the file header. For append commits, the range begins immediately after the previous footer. Historical object records may be referenced through authenticated leaf entries and do not need to be repeated in the latest commit range.

## 9. Physical write order

The first writer uses this deterministic order for each commit:

1. file header, genesis only;
2. newly written object records sorted by object identifier;
3. directory leaf pages in ascending key-range order;
4. internal pages bottom-up and in ascending key-range order within each level;
5. snapshot record;
6. footer.

The first implementation rebuilds all directory pages for every snapshot. Page reuse and copy-on-write updates are later experiments.

## 10. Strict validation order

A strict reader must fail closed and should validate in this order:

1. apply file-size and configured resource limits;
2. require at least one header and one exact-end footer;
3. parse and validate footer structure and reserved bytes;
4. check footer ranges and previous-footer ordering;
5. parse the referenced snapshot and verify its exact length and digest;
6. cross-check sequence and previous-footer fields;
7. parse and authenticate the directory root;
8. traverse pages under independent page, depth, allocation, and read limits;
9. validate key ordering, ranges, levels, references, cycles, zero padding, and page digests;
10. validate root identifiers and capability arrays;
11. validate referenced object headers, locators, non-overlap, and object digests under object and byte limits;
12. compute the commit digest over the exact committed range and footer semantics;
13. require the footer to end at exact file end;
14. publish a verified result only after every required check succeeds.

A reader may reorder independent checks for performance only if it never allocates, decodes, or semantically uses data before the checks required to make that operation safe.

## 11. Append and recovery

### 11.1 Append publication

Bytes before a new footer are unpublished tail state. If writing stops before the footer is complete, strict validation of the new file end fails. Recovery may traverse or discover the earlier complete footer and validate that snapshot independently.

### 11.2 Previous-footer traversal

Given one structurally valid footer, recovery may follow `previous_footer_offset` while enforcing:

- strictly decreasing physical offsets;
- no repeated offsets;
- configured maximum chain depth;
- exact 160-byte footer ranges;
- complete validation of every candidate before reporting it as verified.

A previous-footer pointer is a discovery aid, not proof of validity.

### 11.3 Backward scanning

Backward scanning may search for `UCOF2END`, but must independently bound scan bytes, magic matches, candidate validations, chain depth, and returned results. A magic match alone has no authority.

## 12. Canonical vectors required

The experiment must publish at least:

- empty-payload genesis with one object;
- multi-object, multi-leaf genesis;
- one append reusing old objects and adding a new object;
- every-byte truncation around snapshot and footer publication;
- malformed header, reserved bytes, lengths, ranges, padding, ordering, and levels;
- invalid object, page, snapshot, and commit digests;
- previous-footer cycles and forward pointers;
- missing parent, sequence gap, fork, and stale-root cases;
- candidate-storm recovery cases.

Every valid vector must have byte-for-byte agreement between the Rust and independent Python implementations.

## 13. Exit and retirement

This candidate may advance within FCP-0002 only after deterministic vectors, independent implementations, hostile-input tests, scale measurements, and fuzzing support its assumptions. If page size, fixed entries, identity scope, footer semantics, or recovery rules fail, the candidate is retired and a new experimental byte candidate is assigned rather than silently changing `UCOF-EXP-0002` bytes.
