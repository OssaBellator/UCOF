use sha2::{Digest, Sha256};
use std::collections::HashSet;

const FILE_HEADER_LEN: usize = 64;
const OBJECT_HEADER_LEN: usize = 48;
const PAGE_SIZE: usize = 16 * 1024;
const PAGE_HEADER_LEN: usize = 64;
const LEAF_ENTRY_LEN: usize = 88;
const INTERNAL_ENTRY_LEN: usize = 64;
const SNAPSHOT_LEN: usize = 96;
const FOOTER_LEN: usize = 128;
const ABSENT_OFFSET: u64 = u64::MAX;

const FILE_MAGIC: &[u8; 8] = b"UCOFIM02";
const OBJECT_MAGIC: &[u8; 8] = b"UCOBOBJ2";
const PAGE_MAGIC: &[u8; 8] = b"UCPGIM02";
const SNAPSHOT_MAGIC: &[u8; 8] = b"UCSNIM02";
const FOOTER_MAGIC: &[u8; 8] = b"UCFTIM02";

const OBJECT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-OBJECT\0";
const PAGE_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-PAGE\0";
const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-SNAPSHOT\0";
const COMMIT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-COMMIT\0";

#[derive(Clone, Copy, Debug)]
struct PageRef {
    minimum: u64,
    maximum: u64,
    offset: usize,
    level: u8,
    digest: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
struct Locator {
    object_id: u64,
    kind: u16,
    record_offset: usize,
    record_len: usize,
    logical_len: u64,
    digest: [u8; 32],
}

fn decode_hex(input: &str) -> Vec<u8> {
    let digits: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    assert_eq!(digits.len() % 2, 0, "hex digit count");
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("high hex nibble");
            let low = (pair[1] as char).to_digit(16).expect("low hex nibble");
            ((high << 4) | low) as u8
        })
        .collect()
}

fn checked(bytes: &[u8], offset: usize, length: usize) -> &[u8] {
    bytes
        .get(offset..offset.checked_add(length).expect("range overflow"))
        .expect("range")
}

fn array<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    checked(bytes, offset, N).try_into().expect("fixed field")
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(array(bytes, offset))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(array(bytes, offset))
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(array(bytes, offset))
}

fn usize_at(bytes: &[u8], offset: usize) -> usize {
    usize::try_from(u64_at(bytes, offset)).expect("usize field")
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn parse_page(
    bytes: &[u8],
    reference: PageRef,
    snapshot_offset: usize,
    seen: &mut HashSet<usize>,
    locators: &mut Vec<Locator>,
    structural_ranges: &mut Vec<(usize, usize)>,
) {
    assert!(reference.offset >= FILE_HEADER_LEN, "page before header");
    let page_end = reference
        .offset
        .checked_add(PAGE_SIZE)
        .expect("page range overflow");
    assert!(page_end <= snapshot_offset, "page after snapshot start");
    assert!(seen.insert(reference.offset), "page cycle or duplicate reference");

    let page = checked(bytes, reference.offset, PAGE_SIZE);
    assert_eq!(sha256(&[PAGE_DOMAIN, page]), reference.digest, "page digest");
    assert_eq!(checked(page, 0, 8), PAGE_MAGIC, "page magic");

    let kind = page[8];
    let level = page[9];
    let reserved = u16_at(page, 10);
    let count = usize::try_from(u32_at(page, 12)).expect("entry count");
    let entry_size = usize::try_from(u32_at(page, 16)).expect("entry size");
    let minimum = u64_at(page, 20);
    let maximum = u64_at(page, 28);
    assert_eq!(reserved, 0, "page reserved");
    assert!(checked(page, 36, 28).iter().all(|byte| *byte == 0));
    assert!(count > 0, "empty page");
    assert_eq!(level, reference.level, "page level");
    assert_eq!((minimum, maximum), (reference.minimum, reference.maximum));
    structural_ranges.push((reference.offset, page_end));

    match kind {
        1 => {
            assert_eq!(level, 0, "leaf level");
            assert_eq!(entry_size, LEAF_ENTRY_LEN, "leaf entry size");
            assert!(PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN <= PAGE_SIZE);
            let before = locators.len();
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let object_id = u64_at(page, entry);
                let object_kind = u16_at(page, entry + 8);
                assert!(object_id != 0 && object_kind != 0);
                assert!(checked(page, entry + 10, 6).iter().all(|byte| *byte == 0));
                assert!(checked(page, entry + 72, 16).iter().all(|byte| *byte == 0));
                locators.push(Locator {
                    object_id,
                    kind: object_kind,
                    record_offset: usize_at(page, entry + 16),
                    record_len: usize_at(page, entry + 24),
                    logical_len: u64_at(page, entry + 32),
                    digest: array(page, entry + 40),
                });
            }
            let added = &locators[before..];
            assert!(added
                .windows(2)
                .all(|pair| pair[0].object_id < pair[1].object_id));
            assert_eq!(added.first().expect("leaf first").object_id, minimum);
            assert_eq!(added.last().expect("leaf last").object_id, maximum);
            let used = PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN;
            assert!(page[used..].iter().all(|byte| *byte == 0), "leaf padding");
        }
        2 => {
            assert!(level > 0, "internal level");
            assert_eq!(entry_size, INTERNAL_ENTRY_LEN, "internal entry size");
            assert!(PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN <= PAGE_SIZE);
            let mut children = Vec::with_capacity(count);
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child = PageRef {
                    minimum: u64_at(page, entry),
                    maximum: u64_at(page, entry + 8),
                    offset: usize_at(page, entry + 16),
                    level: level - 1,
                    digest: array(page, entry + 32),
                };
                assert_eq!(usize_at(page, entry + 24), PAGE_SIZE);
                assert!(child.minimum <= child.maximum);
                children.push(child);
            }
            assert!(children
                .windows(2)
                .all(|pair| pair[0].maximum < pair[1].minimum));
            assert_eq!(children.first().expect("child first").minimum, minimum);
            assert_eq!(children.last().expect("child last").maximum, maximum);
            let used = PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN;
            assert!(page[used..].iter().all(|byte| *byte == 0), "internal padding");
            for child in children {
                parse_page(
                    bytes,
                    child,
                    snapshot_offset,
                    seen,
                    locators,
                    structural_ranges,
                );
            }
        }
        _ => panic!("unsupported page kind"),
    }
}

#[test]
fn independently_parses_and_hashes_pinned_immutable_vector() {
    let bytes = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002-immutable/genesis-four.hex"
    ));
    assert_eq!(bytes.len(), 16_886, "pinned vector length");
    assert_eq!(checked(&bytes, 0, 8), FILE_MAGIC);
    assert!(bytes[8..FILE_HEADER_LEN].iter().all(|byte| *byte == 0));

    let footer_offset = bytes.len() - FOOTER_LEN;
    let footer = checked(&bytes, footer_offset, FOOTER_LEN);
    assert_eq!(checked(footer, 0, 8), FOOTER_MAGIC);
    let sequence = u64_at(footer, 8);
    let snapshot_offset = usize_at(footer, 16);
    let snapshot_len = usize_at(footer, 24);
    let previous_footer_offset = u64_at(footer, 32);
    let page_count_current = usize_at(footer, 40);
    let snapshot_digest: [u8; 32] = array(footer, 48);
    let commit_digest: [u8; 32] = array(footer, 80);
    assert!(footer[112..].iter().all(|byte| *byte == 0));
    assert_eq!(sequence, 0);
    assert_eq!(previous_footer_offset, ABSENT_OFFSET);
    assert_eq!(snapshot_len, SNAPSHOT_LEN);
    assert_eq!(snapshot_offset + snapshot_len, footer_offset);

    let snapshot = checked(&bytes, snapshot_offset, snapshot_len);
    assert_eq!(checked(snapshot, 0, 8), SNAPSHOT_MAGIC);
    assert_eq!(u64_at(snapshot, 8), sequence);
    let root_offset = usize_at(snapshot, 16);
    let root_level = u64_at(snapshot, 24);
    let root_digest: [u8; 32] = array(snapshot, 32);
    assert!(snapshot[64..].iter().all(|byte| *byte == 0));
    assert_eq!(sha256(&[SNAPSHOT_DOMAIN, snapshot]), snapshot_digest);

    let mut semantics = Vec::with_capacity(72);
    semantics.extend_from_slice(&sequence.to_le_bytes());
    semantics.extend_from_slice(&(snapshot_offset as u64).to_le_bytes());
    semantics.extend_from_slice(&(snapshot_len as u64).to_le_bytes());
    semantics.extend_from_slice(&previous_footer_offset.to_le_bytes());
    semantics.extend_from_slice(&(page_count_current as u64).to_le_bytes());
    semantics.extend_from_slice(&snapshot_digest);
    assert_eq!(semantics.len(), 72);
    assert_eq!(
        sha256(&[COMMIT_DOMAIN, &bytes[..footer_offset], &semantics]),
        commit_digest,
        "commit digest"
    );

    let root_page = checked(&bytes, root_offset, PAGE_SIZE);
    let root = PageRef {
        minimum: u64_at(root_page, 20),
        maximum: u64_at(root_page, 28),
        offset: root_offset,
        level: u8::try_from(root_level).expect("root level"),
        digest: root_digest,
    };
    let mut seen = HashSet::new();
    let mut locators = Vec::new();
    let mut structural_ranges = vec![
        (snapshot_offset, footer_offset),
        (footer_offset, bytes.len()),
    ];
    parse_page(
        &bytes,
        root,
        snapshot_offset,
        &mut seen,
        &mut locators,
        &mut structural_ranges,
    );
    assert_eq!(seen.len(), page_count_current);
    assert_eq!(locators.len(), 4);
    assert!(locators
        .windows(2)
        .all(|pair| pair[0].object_id < pair[1].object_id));
    assert_eq!(locators.first().expect("first object").object_id, root.minimum);
    assert_eq!(locators.last().expect("last object").object_id, root.maximum);

    let mut object_ranges = Vec::new();
    let mut payloads = Vec::new();
    for locator in &locators {
        let end = locator
            .record_offset
            .checked_add(locator.record_len)
            .expect("object range overflow");
        assert!(locator.record_offset >= FILE_HEADER_LEN && end <= snapshot_offset);
        assert!(!structural_ranges
            .iter()
            .any(|(start, stop)| locator.record_offset < *stop && *start < end));
        object_ranges.push((locator.record_offset, end));

        let record = checked(&bytes, locator.record_offset, locator.record_len);
        assert_eq!(checked(record, 0, 8), OBJECT_MAGIC);
        assert_eq!(usize::from(u16_at(record, 8)), OBJECT_HEADER_LEN);
        assert_eq!(u16_at(record, 10), locator.kind);
        assert_eq!(u32_at(record, 12), 0);
        assert_eq!(u64_at(record, 16), locator.object_id);
        let payload_len = usize_at(record, 24);
        assert_eq!(u64_at(record, 32), locator.logical_len);
        assert_eq!(payload_len as u64, locator.logical_len);
        assert_eq!(OBJECT_HEADER_LEN + payload_len, locator.record_len);
        assert!(record[40..OBJECT_HEADER_LEN]
            .iter()
            .all(|byte| *byte == 0));
        assert_eq!(sha256(&[OBJECT_DOMAIN, record]), locator.digest);
        payloads.push((locator.object_id, record[OBJECT_HEADER_LEN..].to_vec()));
    }
    object_ranges.sort_unstable();
    assert!(object_ranges
        .windows(2)
        .all(|pair| pair[0].1 <= pair[1].0));
    assert_eq!(
        payloads,
        vec![
            (1, b"alpha".to_vec()),
            (2, b"bravo".to_vec()),
            (3, b"charlie".to_vec()),
            (4, b"delta".to_vec()),
        ]
    );

    let file_sha256 = sha256(&[&bytes]);
    println!("immutable_vector_bytes={}", bytes.len());
    println!("immutable_vector_sha256={file_sha256:02x?}");
    println!("independent_rust_parse_and_hash=pass");
}
