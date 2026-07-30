use ucof_experiments::exp0002_lookup::{lookup_authenticated, AuthenticatedLookupLimits};

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

#[test]
fn pinned_multi_leaf_vector_supports_bounded_authenticated_lookup() {
    let bytes = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002/multi-leaf-400.hex"
    ));
    let result = lookup_authenticated(&bytes, 399, &AuthenticatedLookupLimits::default())
        .expect("lookup")
        .expect("object");
    assert_eq!(result.object_id, 399);
    assert_eq!(result.payload_len, 1);
    assert_eq!(result.pages_read, 2);
}

#[test]
fn pinned_append_vector_authenticates_reused_and_new_objects() {
    let bytes = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002/append-add-third.hex"
    ));
    let reused = lookup_authenticated(&bytes, 1, &AuthenticatedLookupLimits::default())
        .expect("lookup reused")
        .expect("reused object");
    let added = lookup_authenticated(&bytes, 3, &AuthenticatedLookupLimits::default())
        .expect("lookup added")
        .expect("added object");
    assert_eq!(reused.sequence, 1);
    assert_eq!(added.sequence, 1);
    assert_eq!(reused.payload_len, 5);
    assert_eq!(added.payload_len, 5);
}

#[test]
fn pinned_vector_proves_absence_without_full_directory_materialisation() {
    let bytes = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002/multi-leaf-400.hex"
    ));
    assert_eq!(
        lookup_authenticated(&bytes, 401, &AuthenticatedLookupLimits::default())
            .expect("absence lookup"),
        None
    );
}
