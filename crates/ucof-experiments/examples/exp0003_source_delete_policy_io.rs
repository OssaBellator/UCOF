use sha2::{Digest, Sha256};
use ucof_experiments::immutable_successor::{
    append_persistent_delete_experimental, append_persistent_insert, build_genesis,
    inspect_persistent_delete_leaf_frontier_experimental, plan_persistent_deletion_tail_at,
    ExperimentalDeleteBorrowDirection, ExperimentalDeleteBorrowPolicy, ImmutableLimits,
    ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableSourceLimits,
    PersistentSourceVersion, PersistentVersionedReadAt, FOOTER_LEN, INTERNAL_ENTRY_LEN,
    LEAF_CAPACITY, LEAF_MIN_OCCUPANCY, PAGE_HEADER_LEN, PAGE_SIZE,
};

const TARGET_OBJECT_ID: u64 = 186;
const REQUEST_CAP: usize = 257;
const PAGE_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-PAGE\0";

struct VersionedSlice {
    bytes: Vec<u8>,
    version: PersistentSourceVersion,
    reads: u64,
}

impl ImmutableReadAt for VersionedSlice {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(
            self.bytes
                .get(start..end)
                .ok_or(ImmutableSourceError::Io("range"))?,
        );
        self.reads = self
            .reads
            .checked_add(1)
            .ok_or(ImmutableSourceError::Limit("read operations"))?;
        Ok(())
    }
}

impl PersistentVersionedReadAt for VersionedSlice {
    fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
        Ok(self.version)
    }
}

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, 1, vec![object_id as u8])
}

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(object)
        .collect()
}

fn comparison_fixture(limits: ImmutableLimits) -> Vec<u8> {
    assert_eq!(LEAF_CAPACITY, 185);
    assert_eq!(LEAF_MIN_OCCUPANCY, 93);

    let mut state = build_genesis(&objects(2 * LEAF_CAPACITY), limits).expect("two full leaves");
    for object_id in u64::try_from(2 * LEAF_CAPACITY + 1).expect("first insertion")..=379 {
        state = append_persistent_insert(&state, &object(object_id), limits)
            .expect("grow right sibling")
            .bytes;
    }

    let left_deletions = LEAF_CAPACITY - (LEAF_MIN_OCCUPANCY + 1);
    assert_eq!(left_deletions, 91);
    for object_id in 1..=u64::try_from(left_deletions).expect("left deletions") {
        state = append_persistent_delete_experimental(
            &state,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        )
        .expect("shrink left sibling")
        .bytes;
    }
    state
}

fn source_limits(format: ImmutableLimits, file_len: usize) -> ImmutableSourceLimits {
    ImmutableSourceLimits {
        format,
        max_total_bytes_read: u64::try_from(file_len * 12).expect("budget"),
        max_read_operations: 2_000_000,
        max_read_request_bytes: REQUEST_CAP,
        hash_block_bytes: 251,
    }
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("u64 field"))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 field"))
}

struct RightSiblingReference {
    offset: usize,
    digest: [u8; 32],
}

fn right_sibling_reference(bytes: &[u8], object_id: u64) -> RightSiblingReference {
    let footer_offset = bytes.len() - FOOTER_LEN;
    let snapshot_offset =
        usize::try_from(u64_at(bytes, footer_offset + 16)).expect("snapshot offset");
    let root_offset = usize::try_from(u64_at(bytes, snapshot_offset + 16)).expect("root offset");
    let root = &bytes[root_offset..root_offset + PAGE_SIZE];
    assert_eq!(root[8], 2, "comparison root must be internal");
    assert_eq!(root[9], 1, "comparison root must be depth one");
    let count = usize::try_from(u32_at(root, 12)).expect("child count");
    assert_eq!(count, 3);

    let child_index = (0..count)
        .position(|index| {
            let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
            let minimum = u64_at(root, entry);
            let maximum = u64_at(root, entry + 8);
            minimum <= object_id && object_id <= maximum
        })
        .expect("target child");
    assert_eq!(child_index, 1, "target must be the middle leaf");
    let right_entry = PAGE_HEADER_LEN + (child_index + 1) * INTERNAL_ENTRY_LEN;
    let offset = usize::try_from(u64_at(root, right_entry + 16)).expect("right offset");
    assert_eq!(
        usize::try_from(u64_at(root, right_entry + 24)).expect("right len"),
        PAGE_SIZE
    );
    let digest = root[right_entry + 32..right_entry + 64]
        .try_into()
        .expect("right digest");
    RightSiblingReference { offset, digest }
}

#[derive(Clone, Copy)]
struct ProbeStats {
    read_operations: u64,
    bytes_read: u64,
    bytes_hashed: u64,
    version_checks: u64,
    occupancy: usize,
}

fn authenticate_page_probe(
    source: &mut VersionedSlice,
    reference: &RightSiblingReference,
) -> ProbeStats {
    let expected_version = source.version_token().expect("initial version");
    let mut page = vec![0_u8; PAGE_SIZE];
    let mut completed = 0_usize;
    let mut read_operations = 0_u64;
    let mut version_checks = 0_u64;
    while completed < page.len() {
        let take = (page.len() - completed).min(REQUEST_CAP);
        assert_eq!(
            source.version_token().expect("pre-read version"),
            expected_version
        );
        version_checks += 1;
        source
            .read_exact_at(
                u64::try_from(reference.offset + completed).expect("offset"),
                &mut page[completed..completed + take],
            )
            .expect("right sibling read");
        assert_eq!(
            source.version_token().expect("post-read version"),
            expected_version
        );
        version_checks += 1;
        completed += take;
        read_operations += 1;
    }

    let mut hasher = Sha256::new();
    hasher.update(PAGE_DOMAIN);
    hasher.update(&page);
    let actual: [u8; 32] = hasher.finalize().into();
    assert_eq!(actual, reference.digest, "right sibling digest");
    let occupancy = usize::try_from(u32_at(&page, 12)).expect("right occupancy");

    ProbeStats {
        read_operations,
        bytes_read: u64::try_from(PAGE_SIZE).expect("page bytes"),
        bytes_hashed: u64::try_from(PAGE_SIZE).expect("page bytes"),
        version_checks,
        occupancy,
    }
}

fn main() {
    let format = ImmutableLimits {
        max_file_bytes: 32 * 1024 * 1024,
        max_output_bytes: 32 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let fixture = comparison_fixture(format);

    let left_frontier = inspect_persistent_delete_leaf_frontier_experimental(
        &fixture,
        TARGET_OBJECT_ID,
        format,
        ExperimentalDeleteBorrowPolicy::LeftFirst,
    )
    .expect("left frontier");
    let fuller_frontier = inspect_persistent_delete_leaf_frontier_experimental(
        &fixture,
        TARGET_OBJECT_ID,
        format,
        ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
    )
    .expect("fuller frontier");
    assert_eq!(left_frontier.target_occupancy, 93);
    assert_eq!(left_frontier.left_occupancy, Some(94));
    assert_eq!(left_frontier.right_occupancy, Some(101));
    assert_eq!(
        left_frontier.selected_donor_direction,
        Some(ExperimentalDeleteBorrowDirection::Left)
    );
    assert_eq!(
        fuller_frontier.selected_donor_direction,
        Some(ExperimentalDeleteBorrowDirection::Right)
    );

    let left_owned = append_persistent_delete_experimental(
        &fixture,
        TARGET_OBJECT_ID,
        format,
        ExperimentalDeleteBorrowPolicy::LeftFirst,
    )
    .expect("left owned");
    let fuller_owned = append_persistent_delete_experimental(
        &fixture,
        TARGET_OBJECT_ID,
        format,
        ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
    )
    .expect("fuller owned");
    assert_ne!(left_owned.bytes, fuller_owned.bytes);
    assert_eq!(left_owned.pages_written, fuller_owned.pages_written);
    assert_eq!(left_owned.pages_reused, fuller_owned.pages_reused);

    let mut source = VersionedSlice {
        bytes: fixture.clone(),
        version: PersistentSourceVersion([0x5a; 32]),
        reads: 0,
    };
    let left_plan = plan_persistent_deletion_tail_at(
        &mut source,
        TARGET_OBJECT_ID,
        source_limits(format, fixture.len()),
    )
    .expect("left source plan");
    assert_eq!(left_plan.tail, left_owned.bytes[fixture.len()..]);
    assert_eq!(source.reads, left_plan.source_stats.read_operations);

    let right_reference = right_sibling_reference(&fixture, TARGET_OBJECT_ID);
    let mut probe_source = VersionedSlice {
        bytes: fixture,
        version: PersistentSourceVersion([0x5a; 32]),
        reads: 0,
    };
    let probe = authenticate_page_probe(&mut probe_source, &right_reference);
    assert_eq!(probe.occupancy, 101);
    assert_eq!(probe.read_operations, 64);
    assert_eq!(probe.bytes_read, 16_384);
    assert_eq!(probe.bytes_hashed, 16_384);
    assert_eq!(probe.version_checks, 128);
    assert_eq!(probe_source.reads, probe.read_operations);

    println!("metric,left_first_source,fuller_information_delta");
    println!(
        "read_operations,{},{}",
        left_plan.source_stats.read_operations, probe.read_operations
    );
    println!(
        "bytes_read,{},{}",
        left_plan.source_stats.bytes_read, probe.bytes_read
    );
    println!(
        "bytes_hashed,{},{}",
        left_plan.source_stats.bytes_hashed, probe.bytes_hashed
    );
    println!(
        "version_checks,{},{}",
        left_plan.version_checks, probe.version_checks
    );
    println!("request_cap_bytes,{REQUEST_CAP},0");
    println!("right_sibling_occupancy,0,{}", probe.occupancy);
    println!("left_pages_written,{},0", left_owned.pages_written);
    println!("fuller_pages_written,{},0", fuller_owned.pages_written);
    println!("persistent_outputs_equal,0,0");
}
