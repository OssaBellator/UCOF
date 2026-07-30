use sha2::{Digest, Sha256};

const FILE_HEADER_LEN: usize = 64;
const OBJECT_HEADER_LEN: usize = 48;
const PAGE_SIZE: usize = 16 * 1024;
const PAGE_HEADER_LEN: usize = 64;
const LEAF_ENTRY_LEN: usize = 88;
const INTERNAL_ENTRY_LEN: usize = 64;
const SNAPSHOT_LEN: usize = 96;
const FOOTER_LEN: usize = 128;
const ABSENT_OFFSET: u64 = u64::MAX;
const LEAF_CAPACITY: usize = (PAGE_SIZE - PAGE_HEADER_LEN) / LEAF_ENTRY_LEN;
const INTERNAL_FANOUT: usize = (PAGE_SIZE - PAGE_HEADER_LEN) / INTERNAL_ENTRY_LEN;

const FILE_MAGIC: &[u8; 8] = b"UCOFIM02";
const OBJECT_MAGIC: &[u8; 8] = b"UCOBOBJ2";
const PAGE_MAGIC: &[u8; 8] = b"UCPGIM02";
const SNAPSHOT_MAGIC: &[u8; 8] = b"UCSNIM02";
const FOOTER_MAGIC: &[u8; 8] = b"UCFTIM02";

const OBJECT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-OBJECT\0";
const PAGE_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-PAGE\0";
const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-SNAPSHOT\0";
const COMMIT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-COMMIT\0";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Locator {
    object_id: u64,
    kind: u16,
    record_offset: u64,
    record_len: u64,
    logical_len: u64,
    digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PageRef {
    minimum: u64,
    maximum: u64,
    offset: u64,
    level: u8,
    digest: [u8; 32],
}

struct Generated {
    bytes: Vec<u8>,
    locators: Vec<Locator>,
    root: PageRef,
    snapshot_digest: [u8; 32],
    page_count: usize,
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn u64_from_usize(value: usize) -> u64 {
    u64::try_from(value).expect("value fits in u64")
}

fn u32_from_usize(value: usize) -> u32 {
    u32::try_from(value).expect("value fits in u32")
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn hex_digest(value: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn decode_hex(input: &str) -> Vec<u8> {
    let digits: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    assert_eq!(digits.len() % 2, 0);
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("high nibble");
            let low = (pair[1] as char).to_digit(16).expect("low nibble");
            ((high << 4) | low) as u8
        })
        .collect()
}

fn encode_object(object_id: u64, kind: u16, payload: &[u8]) -> Vec<u8> {
    assert!(object_id != 0 && kind != 0);
    let mut record = vec![0_u8; OBJECT_HEADER_LEN + payload.len()];
    record[..8].copy_from_slice(OBJECT_MAGIC);
    put_u16(
        &mut record,
        8,
        u16::try_from(OBJECT_HEADER_LEN).expect("header length"),
    );
    put_u16(&mut record, 10, kind);
    put_u64(&mut record, 16, object_id);
    put_u64(&mut record, 24, u64_from_usize(payload.len()));
    put_u64(&mut record, 32, u64_from_usize(payload.len()));
    record[OBJECT_HEADER_LEN..].copy_from_slice(payload);
    record
}

fn append_object(output: &mut Vec<u8>, object_id: u64, kind: u16, payload: &[u8]) -> Locator {
    let record = encode_object(object_id, kind, payload);
    let offset = u64_from_usize(output.len());
    output.extend_from_slice(&record);
    Locator {
        object_id,
        kind,
        record_offset: offset,
        record_len: u64_from_usize(record.len()),
        logical_len: u64_from_usize(payload.len()),
        digest: sha256(&[OBJECT_DOMAIN, &record]),
    }
}

fn encode_leaf(entries: &[Locator]) -> Vec<u8> {
    assert!(!entries.is_empty() && entries.len() <= LEAF_CAPACITY);
    assert!(entries
        .windows(2)
        .all(|pair| pair[0].object_id < pair[1].object_id));

    let mut page = vec![0_u8; PAGE_SIZE];
    page[..8].copy_from_slice(PAGE_MAGIC);
    page[8] = 1;
    put_u32(&mut page, 12, u32_from_usize(entries.len()));
    put_u32(&mut page, 16, u32_from_usize(LEAF_ENTRY_LEN));
    put_u64(&mut page, 20, entries[0].object_id);
    put_u64(
        &mut page,
        28,
        entries.last().expect("last leaf entry").object_id,
    );

    for (index, entry) in entries.iter().enumerate() {
        let offset = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
        put_u64(&mut page, offset, entry.object_id);
        put_u16(&mut page, offset + 8, entry.kind);
        put_u64(&mut page, offset + 16, entry.record_offset);
        put_u64(&mut page, offset + 24, entry.record_len);
        put_u64(&mut page, offset + 32, entry.logical_len);
        page[offset + 40..offset + 72].copy_from_slice(&entry.digest);
    }
    page
}

fn encode_internal(children: &[PageRef], level: u8) -> Vec<u8> {
    assert!(!children.is_empty() && children.len() <= INTERNAL_FANOUT && level > 0);
    assert!(children
        .windows(2)
        .all(|pair| pair[0].maximum < pair[1].minimum));
    assert!(children.iter().all(|child| child.level + 1 == level));

    let mut page = vec![0_u8; PAGE_SIZE];
    page[..8].copy_from_slice(PAGE_MAGIC);
    page[8] = 2;
    page[9] = level;
    put_u32(&mut page, 12, u32_from_usize(children.len()));
    put_u32(&mut page, 16, u32_from_usize(INTERNAL_ENTRY_LEN));
    put_u64(&mut page, 20, children[0].minimum);
    put_u64(&mut page, 28, children.last().expect("last child").maximum);

    for (index, child) in children.iter().enumerate() {
        let offset = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
        put_u64(&mut page, offset, child.minimum);
        put_u64(&mut page, offset + 8, child.maximum);
        put_u64(&mut page, offset + 16, child.offset);
        put_u64(&mut page, offset + 24, u64_from_usize(PAGE_SIZE));
        page[offset + 32..offset + 64].copy_from_slice(&child.digest);
    }
    page
}

fn append_page(output: &mut Vec<u8>, page: &[u8]) -> PageRef {
    assert_eq!(page.len(), PAGE_SIZE);
    let reference = PageRef {
        minimum: u64::from_le_bytes(page[20..28].try_into().expect("page minimum")),
        maximum: u64::from_le_bytes(page[28..36].try_into().expect("page maximum")),
        offset: u64_from_usize(output.len()),
        level: page[9],
        digest: sha256(&[PAGE_DOMAIN, page]),
    };
    output.extend_from_slice(page);
    reference
}

fn build_tree(output: &mut Vec<u8>, locators: &mut [Locator]) -> PageRef {
    locators.sort_by_key(|locator| locator.object_id);
    assert!(locators
        .windows(2)
        .all(|pair| pair[0].object_id < pair[1].object_id));

    let mut level: Vec<PageRef> = locators
        .chunks(LEAF_CAPACITY)
        .map(|chunk| append_page(output, &encode_leaf(chunk)))
        .collect();
    while level.len() > 1 {
        let mut next = Vec::new();
        for chunk in level.chunks(INTERNAL_FANOUT) {
            next.push(append_page(
                output,
                &encode_internal(chunk, chunk[0].level + 1),
            ));
        }
        level = next;
    }
    level.pop().expect("root page")
}

fn footer_semantics(
    sequence: u64,
    snapshot_offset: u64,
    previous_footer_offset: u64,
    page_count: usize,
    snapshot_digest: &[u8; 32],
) -> Vec<u8> {
    let mut semantics = vec![0_u8; 72];
    put_u64(&mut semantics, 0, sequence);
    put_u64(&mut semantics, 8, snapshot_offset);
    put_u64(&mut semantics, 16, u64_from_usize(SNAPSHOT_LEN));
    put_u64(&mut semantics, 24, previous_footer_offset);
    put_u64(&mut semantics, 32, u64_from_usize(page_count));
    semantics[40..].copy_from_slice(snapshot_digest);
    semantics
}

fn publish(
    output: &mut Vec<u8>,
    sequence: u64,
    root: &PageRef,
    parent_snapshot_digest: &[u8; 32],
    previous_footer_offset: u64,
    page_count: usize,
) -> [u8; 32] {
    let snapshot_offset = u64_from_usize(output.len());
    let mut snapshot = vec![0_u8; SNAPSHOT_LEN];
    snapshot[..8].copy_from_slice(SNAPSHOT_MAGIC);
    put_u64(&mut snapshot, 8, sequence);
    put_u64(&mut snapshot, 16, root.offset);
    put_u64(&mut snapshot, 24, u64::from(root.level));
    snapshot[32..64].copy_from_slice(&root.digest);
    snapshot[64..].copy_from_slice(parent_snapshot_digest);
    let snapshot_digest = sha256(&[SNAPSHOT_DOMAIN, &snapshot]);
    output.extend_from_slice(&snapshot);

    let semantics = footer_semantics(
        sequence,
        snapshot_offset,
        previous_footer_offset,
        page_count,
        &snapshot_digest,
    );
    let commit_start = if previous_footer_offset == ABSENT_OFFSET {
        0
    } else {
        usize::try_from(previous_footer_offset).expect("footer offset") + FOOTER_LEN
    };
    let commit_digest = sha256(&[COMMIT_DOMAIN, &output[commit_start..], &semantics]);

    let mut footer = vec![0_u8; FOOTER_LEN];
    footer[..8].copy_from_slice(FOOTER_MAGIC);
    put_u64(&mut footer, 8, sequence);
    put_u64(&mut footer, 16, snapshot_offset);
    put_u64(&mut footer, 24, u64_from_usize(SNAPSHOT_LEN));
    put_u64(&mut footer, 32, previous_footer_offset);
    put_u64(&mut footer, 40, u64_from_usize(page_count));
    footer[48..80].copy_from_slice(&snapshot_digest);
    footer[80..112].copy_from_slice(&commit_digest);
    output.extend_from_slice(&footer);
    snapshot_digest
}

fn build_genesis(values: &[(u64, u16, Vec<u8>)]) -> Generated {
    let mut output = vec![0_u8; FILE_HEADER_LEN];
    output[..8].copy_from_slice(FILE_MAGIC);
    let mut locators: Vec<Locator> = values
        .iter()
        .map(|(object_id, kind, payload)| append_object(&mut output, *object_id, *kind, payload))
        .collect();
    let page_start = output.len();
    let root = build_tree(&mut output, &mut locators);
    let page_count = (output.len() - page_start) / PAGE_SIZE;
    let snapshot_digest = publish(
        &mut output,
        0,
        &root,
        &[0_u8; 32],
        ABSENT_OFFSET,
        page_count,
    );
    Generated {
        bytes: output,
        locators,
        root,
        snapshot_digest,
        page_count,
    }
}

#[test]
fn independently_generates_pinned_successor_recipes() {
    let base_values = vec![
        (1, 1, b"alpha".to_vec()),
        (2, 2, b"bravo".to_vec()),
        (3, 3, b"charlie".to_vec()),
        (4, 1, b"delta".to_vec()),
    ];
    let base = build_genesis(&base_values);
    let pinned = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002-immutable/genesis-four.hex"
    ));
    assert_eq!(base.bytes, pinned);
    assert_eq!(base.bytes.len(), 16_886);
    assert_eq!(
        hex_digest(sha256(&[&base.bytes])),
        "94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23"
    );
    assert_eq!(base.root.level, 0);
    assert_eq!(base.page_count, 1);

    let mut append_bytes = base.bytes.clone();
    let replacement = append_object(&mut append_bytes, 1, 9, b"alpha-v2");
    let mut active = base.locators.clone();
    active[0] = replacement;
    let page_start = append_bytes.len();
    let append_root = build_tree(&mut append_bytes, &mut active);
    let append_page_count = (append_bytes.len() - page_start) / PAGE_SIZE;
    publish(
        &mut append_bytes,
        1,
        &append_root,
        &base.snapshot_digest,
        u64_from_usize(base.bytes.len() - FOOTER_LEN),
        append_page_count,
    );
    assert_eq!(append_bytes.len(), 33_550);
    assert_eq!(append_root.level, 0);
    assert_eq!(append_page_count, 1);
    assert_eq!(
        hex_digest(sha256(&[&append_bytes])),
        "e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc"
    );

    let multi_values: Vec<(u64, u16, Vec<u8>)> = (1_u64..=400)
        .map(|object_id| {
            (
                object_id,
                u16::try_from(1 + object_id % 5).expect("object kind"),
                format!("payload:{object_id}").into_bytes(),
            )
        })
        .collect();
    let multi = build_genesis(&multi_values);
    assert_eq!(multi.bytes.len(), 89_316);
    assert_eq!(multi.root.level, 1);
    assert_eq!(multi.page_count, 4);
    assert_eq!(multi.locators.len(), 400);
    assert_eq!(
        hex_digest(sha256(&[&multi.bytes])),
        "d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70"
    );

    println!("base_cross_language_bytes=pass");
    println!("append_cross_language_sha256=pass");
    println!("multi_level_cross_language_sha256=pass");
}
