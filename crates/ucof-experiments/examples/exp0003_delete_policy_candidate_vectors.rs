use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use ucof_experiments::immutable_successor::{
    append_persistent_delete_experimental, append_persistent_insert, build_genesis, rewrite_all,
    validate_canonical_occupancy, ExperimentalDeleteBorrowPolicy, ImmutableLimits,
    ImmutableObjectInput, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY,
};

const TARGET_OBJECT_ID: u64 = 186;

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

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("string write");
    }
    output
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn render() -> String {
    let limits = ImmutableLimits::default();
    let fixture = comparison_fixture(limits);
    let fixture_report = validate_canonical_occupancy(&fixture, limits).expect("fixture canonical");
    assert_eq!(fixture_report.object_count, 288);

    let left = append_persistent_delete_experimental(
        &fixture,
        TARGET_OBJECT_ID,
        limits,
        ExperimentalDeleteBorrowPolicy::LeftFirst,
    )
    .expect("left-first candidate");
    let fuller = append_persistent_delete_experimental(
        &fixture,
        TARGET_OBJECT_ID,
        limits,
        ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
    )
    .expect("fuller-sibling candidate");

    assert_ne!(left.bytes, fuller.bytes);
    assert_ne!(left.report.snapshot_digest, fuller.report.snapshot_digest);
    assert_eq!(left.report.object_count, fuller.report.object_count);
    assert_eq!(left.report.page_count, fuller.report.page_count);
    assert_eq!(left.report.root_level, fuller.report.root_level);
    assert_eq!(left.pages_written, fuller.pages_written);
    assert_eq!(left.pages_reused, fuller.pages_reused);

    let left_fresh = rewrite_all(&left.bytes, limits).expect("left fresh rewrite");
    let fuller_fresh = rewrite_all(&fuller.bytes, limits).expect("fuller fresh rewrite");
    assert_eq!(
        left_fresh.retained_object_ids,
        fuller_fresh.retained_object_ids
    );
    assert_eq!(left_fresh.bytes, fuller_fresh.bytes);

    let mut output = String::new();
    writeln!(&mut output, "format_version=1").expect("write manifest");
    writeln!(
        &mut output,
        "status=non-normative-candidate-review-evidence"
    )
    .expect("write manifest");
    writeln!(&mut output, "leaf_capacity={LEAF_CAPACITY}").expect("write manifest");
    writeln!(&mut output, "leaf_minimum={LEAF_MIN_OCCUPANCY}").expect("write manifest");
    writeln!(
        &mut output,
        "fixture_recipe=genesis-370;insert-371..379;delete-1..91-left-first"
    )
    .expect("write manifest");
    writeln!(&mut output, "fixture_bytes={}", fixture.len()).expect("write manifest");
    writeln!(&mut output, "fixture_sha256={}", sha256(&fixture)).expect("write manifest");
    writeln!(&mut output, "fixture_sequence={}", fixture_report.sequence).expect("write manifest");
    writeln!(
        &mut output,
        "fixture_snapshot_digest={}",
        hex(&fixture_report.snapshot_digest)
    )
    .expect("write manifest");
    writeln!(
        &mut output,
        "fixture_object_count={}",
        fixture_report.object_count
    )
    .expect("write manifest");
    writeln!(&mut output, "target_object_id={TARGET_OBJECT_ID}").expect("write manifest");

    for (name, result) in [("left-first", &left), ("fuller-sibling", &fuller)] {
        writeln!(&mut output, "{name}.output_bytes={}", result.bytes.len())
            .expect("write manifest");
        writeln!(
            &mut output,
            "{name}.output_sha256={}",
            sha256(&result.bytes)
        )
        .expect("write manifest");
        writeln!(&mut output, "{name}.sequence={}", result.report.sequence)
            .expect("write manifest");
        writeln!(
            &mut output,
            "{name}.snapshot_digest={}",
            hex(&result.report.snapshot_digest)
        )
        .expect("write manifest");
        writeln!(
            &mut output,
            "{name}.object_count={}",
            result.report.object_count
        )
        .expect("write manifest");
        writeln!(
            &mut output,
            "{name}.page_count={}",
            result.report.page_count
        )
        .expect("write manifest");
        writeln!(
            &mut output,
            "{name}.root_level={}",
            result.report.root_level
        )
        .expect("write manifest");
        writeln!(&mut output, "{name}.pages_written={}", result.pages_written)
            .expect("write manifest");
        writeln!(&mut output, "{name}.pages_reused={}", result.pages_reused)
            .expect("write manifest");
    }

    writeln!(
        &mut output,
        "canonical_fresh_bytes={}",
        left_fresh.bytes.len()
    )
    .expect("write manifest");
    writeln!(
        &mut output,
        "canonical_fresh_sha256={}",
        sha256(&left_fresh.bytes)
    )
    .expect("write manifest");
    writeln!(
        &mut output,
        "canonical_retained_object_count={}",
        left_fresh.retained_object_ids.len()
    )
    .expect("write manifest");
    writeln!(&mut output, "persistent_outputs_equal=0").expect("write manifest");
    writeln!(&mut output, "snapshot_digests_equal=0").expect("write manifest");
    writeln!(&mut output, "canonical_fresh_bytes_equal=1").expect("write manifest");
    output
}

fn verify(path: &Path, rendered: &str) {
    let expected = fs::read_to_string(path).expect("read candidate manifest");
    assert_eq!(expected, rendered, "candidate vector manifest drifted");
    println!(
        "verified_candidate_delete_policy_manifest={}",
        path.display()
    );
}

fn main() {
    let rendered = render();
    let mut arguments = std::env::args_os().skip(1);
    match arguments.next() {
        None => print!("{rendered}"),
        Some(flag) if flag == "--verify" => {
            let path = arguments.next().expect("--verify requires a manifest path");
            assert!(arguments.next().is_none(), "unexpected extra argument");
            verify(Path::new(&path), &rendered);
        }
        Some(other) => panic!("unknown argument: {}", other.to_string_lossy()),
    }
}
