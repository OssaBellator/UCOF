#!/usr/bin/env python3
from pathlib import Path

source_path = Path("crates/ucof-experiments/src/immutable_successor/source.rs")
source = source_path.read_text(encoding="utf-8")

trait_old = """pub trait ImmutableReadAt {
    fn len(&mut self) -> Result<u64, ImmutableSourceError>;
"""
trait_new = """/// Minimal synchronous random-access contract for successor evidence.
///
/// Implementations must fill each requested buffer completely or return an error.
/// The caller must also provide one stable source view for the entire operation;
/// conditional remote adapters must restart rather than mix version tokens.
pub trait ImmutableReadAt {
    fn len(&mut self) -> Result<u64, ImmutableSourceError>;
"""
if trait_old not in source:
    raise SystemExit("trait insertion point not found")
source = source.replace(trait_old, trait_new, 1)

method_marker = """    fn read_into(
"""
method = """    fn add_hashed(&mut self, length: usize) -> Result<(), ImmutableSourceError> {
        self.stats.bytes_hashed = self
            .stats
            .bytes_hashed
            .checked_add(
                u64::try_from(length)
                    .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
            )
            .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
        Ok(())
    }

"""
if method_marker not in source:
    raise SystemExit("hash helper insertion point not found")
source = source.replace(method_marker, method + method_marker, 1)

for old, new in [
    ("""    reader.stats.bytes_hashed += u64::try_from(snapshot.len())
        .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?;
""", """    reader.add_hashed(snapshot.len())?;
"""),
    ("""        if footer.sequence != previous.sequence + 1
            || previous.snapshot_digest != parent_snapshot_digest
""", """        if previous.sequence.checked_add(1) != Some(footer.sequence)
            || previous.snapshot_digest != parent_snapshot_digest
"""),
    ("""    if !known_ranges
        .iter()
        .any(|range| *range == (reference.offset, reference.offset + PAGE_SIZE))
    {
        register_page_range(known_ranges, reference.offset, envelope.snapshot_offset)?;
    }
""", """    let page_end = reference
        .offset
        .checked_add(PAGE_SIZE)
        .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page range",
        )))?;
    if !known_ranges
        .iter()
        .any(|range| *range == (reference.offset, page_end))
    {
        register_page_range(known_ranges, reference.offset, envelope.snapshot_offset)?;
    }
"""),
    ("""    reader.stats.bytes_hashed += u64::try_from(page.len())
        .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?;
""", """    reader.add_hashed(page.len())?;
"""),
    ("""    reader.stats.bytes_hashed += u64::try_from(header.len())
        .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?;
""", """    reader.add_hashed(header.len())?;
"""),
]:
    if old not in source:
        raise SystemExit(f"source replacement not found: {old[:60]}")
    source = source.replace(old, new, 1)
source_path.write_text(source, encoding="utf-8")

test_path = Path("crates/ucof-experiments/tests/immutable_successor_source.rs")
test = test_path.read_text(encoding="utf-8")
test = test.replace("use std::io::Cursor;\n", "use std::io::Cursor;\n\nuse sha2::{Digest, Sha256};\n", 1)
append = r'''

const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-SNAPSHOT\0";
const COMMIT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-COMMIT\0";
const FOOTER_LEN: usize = 128;
const SNAPSHOT_LEN: usize = 96;

fn reauthenticate_current_commit(bytes: &mut [u8]) {
    let footer_offset = bytes.len() - FOOTER_LEN;
    let snapshot_offset = footer_offset - SNAPSHOT_LEN;
    let snapshot_digest: [u8; 32] = Sha256::new()
        .chain_update(SNAPSHOT_DOMAIN)
        .chain_update(&bytes[snapshot_offset..footer_offset])
        .finalize()
        .into();
    bytes[footer_offset + 48..footer_offset + 80].copy_from_slice(&snapshot_digest);

    let previous_footer = u64::from_le_bytes(
        bytes[footer_offset + 32..footer_offset + 40]
            .try_into()
            .expect("previous footer"),
    );
    let commit_start = if previous_footer == u64::MAX {
        0
    } else {
        usize::try_from(previous_footer).expect("footer offset") + FOOTER_LEN
    };
    let commit_digest: [u8; 32] = Sha256::new()
        .chain_update(COMMIT_DOMAIN)
        .chain_update(&bytes[commit_start..footer_offset])
        .chain_update(&bytes[footer_offset + 8..footer_offset + 80])
        .finalize()
        .into();
    bytes[footer_offset + 80..footer_offset + 112].copy_from_slice(&commit_digest);
}

#[test]
fn hostile_root_offset_is_rejected_without_unchecked_addition() {
    let mut bytes = small_genesis();
    let snapshot_offset = bytes.len() - FOOTER_LEN - SNAPSHOT_LEN;
    bytes[snapshot_offset + 16..snapshot_offset + 24].copy_from_slice(&u64::MAX.to_le_bytes());
    reauthenticate_current_commit(&mut bytes);

    let mut source = ImmutableSliceSource::new(&bytes);
    assert_eq!(
        lookup_at(&mut source, 1, ImmutableSourceLimits::default()),
        Err(ImmutableSourceError::Format(
            ucof_experiments::immutable_successor::ImmutableError::Invalid("page range")
        ))
    );
}

#[test]
fn overflowing_previous_sequence_is_rejected_as_linkage() {
    let genesis = small_genesis();
    let mut appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(1, 9, b"alpha-v2".to_vec()),
        ImmutableSourceLimits::default().format,
    )
    .expect("append");
    let previous_footer = genesis.len() - FOOTER_LEN;
    appended[previous_footer + 8..previous_footer + 16]
        .copy_from_slice(&u64::MAX.to_le_bytes());

    let footer_offset = appended.len() - FOOTER_LEN;
    let snapshot_offset = footer_offset - SNAPSHOT_LEN;
    appended[snapshot_offset + 8..snapshot_offset + 16].copy_from_slice(&0_u64.to_le_bytes());
    appended[footer_offset + 8..footer_offset + 16].copy_from_slice(&0_u64.to_le_bytes());
    reauthenticate_current_commit(&mut appended);

    let mut source = ImmutableSliceSource::new(&appended);
    assert_eq!(
        lookup_at(&mut source, 1, ImmutableSourceLimits::default()),
        Err(ImmutableSourceError::Format(
            ucof_experiments::immutable_successor::ImmutableError::Invalid("parent linkage")
        ))
    );
}
'''
if "hostile_root_offset_is_rejected_without_unchecked_addition" in test:
    raise SystemExit("hardening tests already present")
test_path.write_text(test + append, encoding="utf-8")
