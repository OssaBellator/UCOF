use ucof_experiments::exp0002::{
    build_append, build_genesis, validate_strict, FileHeader, ObjectInput, ValidationLimits,
};

fn decode_hex(text: &str) -> Vec<u8> {
    let text = text.trim();
    assert_eq!(text.len() % 2, 0, "hex vector has an odd length");
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("hex high nibble");
            let low = (pair[1] as char).to_digit(16).expect("hex low nibble");
            u8::try_from((high << 4) | low).expect("hex byte")
        })
        .collect()
}

fn header() -> FileHeader {
    FileHeader {
        file_id: *b"exp0002-file-id!",
        creation_nonce: *b"fixed-nonce-0002",
    }
}

fn object(id: u64, payload: Vec<u8>, is_root: bool) -> ObjectInput {
    ObjectInput {
        object_id: id,
        kind: 1,
        payload,
        is_root,
    }
}

#[test]
fn rust_matches_python_genesis_vector() {
    let expected = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002/genesis-two-object.hex"
    ));
    let actual = build_genesis(
        header(),
        vec![
            object(2, b"second".to_vec(), false),
            object(1, b"first".to_vec(), true),
        ],
    )
    .expect("genesis");
    assert_eq!(actual, expected);
    validate_strict(&expected, &ValidationLimits::default()).expect("valid vector");
}

#[test]
fn rust_matches_python_append_vector() {
    let genesis = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002/genesis-two-object.hex"
    ));
    let expected = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002/append-add-third.hex"
    ));
    let actual = build_append(
        &genesis,
        vec![object(3, b"third".to_vec(), false)],
        vec![1, 3],
        &ValidationLimits::default(),
    )
    .expect("append");
    assert_eq!(actual, expected);
    validate_strict(&expected, &ValidationLimits::default()).expect("valid vector");
}

#[test]
fn rust_matches_python_multi_leaf_vector() {
    let expected = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002/multi-leaf-400.hex"
    ));
    let objects = (1_u64..=400)
        .map(|id| {
            object(
                id,
                vec![u8::try_from(id % 251).expect("bounded byte")],
                id == 1,
            )
        })
        .collect();
    let actual = build_genesis(header(), objects).expect("multi-leaf genesis");
    assert_eq!(actual, expected);
    let report = validate_strict(&expected, &ValidationLimits::default()).expect("valid vector");
    assert_eq!(report.objects.len(), 400);
    assert!(report.pages_verified > 1);
}
