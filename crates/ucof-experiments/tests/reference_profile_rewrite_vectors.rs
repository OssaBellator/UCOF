use sha2::{Digest, Sha256};
use ucof_experiments::immutable_successor::{
    build_genesis, encode_canonical_reference_list, semantic_compact,
    CanonicalReferenceListResolver, ImmutableCompactionLimits, ImmutableLimits,
    ImmutableObjectInput, UnknownDependencyPolicy,
};

const REFERENCE_KIND: u16 = 100;
const LEAF_KIND: u16 = 101;
const MAX_DEPENDENCIES: usize = 8;

fn reference(object_id: u64, dependencies: &[u64]) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        REFERENCE_KIND,
        encode_canonical_reference_list(dependencies, MAX_DEPENDENCIES)
            .expect("canonical reference payload"),
    )
}

fn leaf(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, LEAF_KIND, Vec::new())
}

fn verify_case(
    name: &str,
    objects: Vec<ImmutableObjectInput>,
    roots: &[u64],
    retained: &[u64],
    discarded: &[u64],
    edges: usize,
    depth: usize,
    expected_len: usize,
    expected_sha256: &str,
) -> [u8; 32] {
    let limits = ImmutableLimits::default();
    let source = build_genesis(&objects, limits).expect("canonical source");
    let mut resolver =
        CanonicalReferenceListResolver::new(REFERENCE_KIND, LEAF_KIND, MAX_DEPENDENCIES)
            .expect("resolver");
    let result = semantic_compact(
        &source,
        roots,
        &mut resolver,
        UnknownDependencyPolicy::Reject,
        ImmutableCompactionLimits::default(),
        limits,
    )
    .expect("profile rewrite");

    assert_eq!(result.retained_object_ids, retained, "{name}: retained");
    assert_eq!(result.discarded_object_ids, discarded, "{name}: discarded");
    assert_eq!(result.edges_visited, edges, "{name}: edges");
    assert_eq!(result.maximum_depth, depth, "{name}: depth");
    assert_eq!(result.rewrite.bytes.len(), expected_len, "{name}: length");
    assert_eq!(result.rewrite.output.root_level, 0, "{name}: root level");
    assert_eq!(result.rewrite.output.page_count, 1, "{name}: pages");
    assert_eq!(
        result.rewrite.output.object_count,
        retained.len(),
        "{name}: object count"
    );

    let digest: [u8; 32] = Sha256::digest(&result.rewrite.bytes).into();
    assert_eq!(hex(&digest), expected_sha256, "{name}: SHA-256");

    let mut reversed = roots.to_vec();
    reversed.reverse();
    let mut reversed_resolver =
        CanonicalReferenceListResolver::new(REFERENCE_KIND, LEAF_KIND, MAX_DEPENDENCIES)
            .expect("resolver");
    let reversed_result = semantic_compact(
        &source,
        &reversed,
        &mut reversed_resolver,
        UnknownDependencyPolicy::Reject,
        ImmutableCompactionLimits::default(),
        limits,
    )
    .expect("reversed profile rewrite");
    assert_eq!(reversed_result.rewrite.bytes, result.rewrite.bytes);
    digest
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[test]
fn rust_matches_independent_reference_profile_rewrite_pins() {
    let cases = [
        (
            "single-root-chain",
            verify_case(
                "single-root-chain",
                vec![
                    reference(1, &[2, 3]),
                    leaf(2),
                    reference(3, &[4]),
                    leaf(4),
                    leaf(5),
                ],
                &[1],
                &[1, 2, 3, 4],
                &[5],
                3,
                2,
                16_904,
                "87bc7de2d5e2afb51e765bf4694cf3a9a178a605d0f032b8157a4bc1bfc7040e",
            ),
        ),
        (
            "two-roots-shared-dependency",
            verify_case(
                "two-roots-shared-dependency",
                vec![
                    reference(10, &[20, 30]),
                    reference(20, &[40]),
                    leaf(30),
                    leaf(40),
                    reference(50, &[40]),
                    leaf(60),
                ],
                &[50, 10],
                &[10, 20, 30, 40, 50],
                &[60],
                4,
                2,
                16_968,
                "aee59b41b6a7bf135fc1d741256ea7e6ef121ffb1897a39b646ca2f66b73b715",
            ),
        ),
        (
            "empty-reference-root",
            verify_case(
                "empty-reference-root",
                vec![reference(7, &[]), leaf(8)],
                &[7],
                &[7],
                &[8],
                0,
                0,
                16_728,
                "65654ef62675f12db1ed3fde4304eaa1344e2cc25bbb5a2718c51ba3779c5e43",
            ),
        ),
    ];

    let mut aggregate = Sha256::new();
    for (name, digest) in cases {
        aggregate.update(name.as_bytes());
        aggregate.update(digest);
    }
    assert_eq!(
        hex(&aggregate.finalize()),
        "fe20a891b04b90b6df1870e6652eec5d6ddfa91ebc8370f4d6dfa70881a27c84"
    );
}
