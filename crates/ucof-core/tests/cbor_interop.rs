use ciborium::value::Value as ExternalValue;
use ucof_core::{decode_canonical, encode_canonical, CborValue, Limits};

fn external_encode(value: &ExternalValue) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("ciborium encoding");
    bytes
}

fn external_unsigned(value: u64) -> ExternalValue {
    ExternalValue::Integer(value.into())
}

#[test]
fn primitive_encodings_match_ciborium() {
    for value in [
        0,
        1,
        23,
        24,
        255,
        256,
        65_535,
        65_536,
        u64::from(u32::MAX),
        u64::from(u32::MAX) + 1,
        u64::MAX,
    ] {
        let ours = encode_canonical(&CborValue::Unsigned(value)).expect("UCOF encoding");
        let external = external_encode(&external_unsigned(value));
        assert_eq!(ours, external, "unsigned value {value}");
    }

    let cases = [
        (
            CborValue::Bytes(vec![0, 1, 2, 255]),
            ExternalValue::Bytes(vec![0, 1, 2, 255]),
        ),
        (
            CborValue::Text("UCOF".to_owned()),
            ExternalValue::Text("UCOF".to_owned()),
        ),
        (CborValue::Bool(false), ExternalValue::Bool(false)),
        (CborValue::Bool(true), ExternalValue::Bool(true)),
        (CborValue::Null, ExternalValue::Null),
        (
            CborValue::Array(vec![
                CborValue::Unsigned(1),
                CborValue::Text("two".to_owned()),
            ]),
            ExternalValue::Array(vec![
                external_unsigned(1),
                ExternalValue::Text("two".to_owned()),
            ]),
        ),
    ];

    for (ours, external) in cases {
        assert_eq!(
            encode_canonical(&ours).expect("UCOF encoding"),
            external_encode(&external)
        );
    }
}

#[test]
fn canonical_map_order_matches_when_external_order_is_explicit() {
    let ours = CborValue::Map(vec![
        (CborValue::Text("aa".to_owned()), CborValue::Unsigned(2)),
        (CborValue::Text("z".to_owned()), CborValue::Unsigned(1)),
    ]);
    let external = ExternalValue::Map(vec![
        (ExternalValue::Text("z".to_owned()), external_unsigned(1)),
        (ExternalValue::Text("aa".to_owned()), external_unsigned(2)),
    ]);

    assert_eq!(
        encode_canonical(&ours).expect("UCOF encoding"),
        external_encode(&external)
    );
}

#[test]
fn ucof_rejects_general_cbor_forms_that_ciborium_accepts() {
    let non_shortest_unsigned = [0x18, 0x17];
    let indefinite_array = [0x9f, 0x01, 0xff];

    let _: ExternalValue =
        ciborium::from_reader(non_shortest_unsigned.as_slice()).expect("general CBOR accepts it");
    let _: ExternalValue =
        ciborium::from_reader(indefinite_array.as_slice()).expect("general CBOR accepts it");

    assert!(decode_canonical(&non_shortest_unsigned, &Limits::default()).is_err());
    assert!(decode_canonical(&indefinite_array, &Limits::default()).is_err());
}
