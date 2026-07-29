use proptest::collection::vec;
use proptest::prelude::*;
use ucof_core::{Manifest, SliceSource, SourceValidator, StreamingWriter, Writer};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn streaming_and_in_memory_writers_are_deterministically_equivalent(
        payloads in vec(vec(any::<u8>(), 0..257), 1..9),
    ) {
        let expected = build_in_memory(&payloads);
        let actual = build_streaming(&payloads);
        prop_assert_eq!(&actual, &expected);

        let mut source = SliceSource::new(&actual);
        let report = SourceValidator::default().validate(&mut source);
        prop_assert!(report.is_ok());
    }

    #[test]
    fn every_nonempty_truncation_is_rejected(
        payload in vec(any::<u8>(), 0..1025),
        cut_selector in any::<u64>(),
    ) {
        let bytes = build_in_memory(&[payload]);
        let cut = usize::try_from(cut_selector % u64::try_from(bytes.len()).expect("file length"))
            .expect("cut index");
        let truncated = &bytes[..cut];
        let mut source = SliceSource::new(truncated);
        prop_assert!(SourceValidator::default().validate(&mut source).is_err());
    }

    #[test]
    fn payload_mutation_never_produces_verified_output(
        payload in vec(any::<u8>(), 1..1025),
        selector in any::<u64>(),
    ) {
        let mut bytes = build_in_memory(&[payload.clone()]);
        let index = 32 + 40 + usize::try_from(
            selector % u64::try_from(payload.len()).expect("payload length")
        ).expect("payload index");
        bytes[index] ^= 1;
        let mut source = SliceSource::new(&bytes);
        prop_assert!(SourceValidator::default().validate(&mut source).is_err());
    }
}

fn build_in_memory(payloads: &[Vec<u8>]) -> Vec<u8> {
    let mut writer = Writer::new();
    for (index, payload) in payloads.iter().enumerate() {
        writer
            .add_opaque(u64::try_from(index + 1).expect("object id"), payload)
            .expect("opaque object");
    }
    let manifest_id = u64::try_from(payloads.len() + 1).expect("manifest id");
    let roots = (1..manifest_id).collect();
    writer
        .add_manifest(manifest_id, &Manifest::new(roots))
        .expect("manifest");
    writer.finish(manifest_id).expect("file")
}

fn build_streaming(payloads: &[Vec<u8>]) -> Vec<u8> {
    let mut writer = StreamingWriter::with_default_limits(Vec::new()).expect("streaming writer");
    for (index, payload) in payloads.iter().enumerate() {
        writer
            .add_opaque(u64::try_from(index + 1).expect("object id"), payload)
            .expect("opaque object");
    }
    let manifest_id = u64::try_from(payloads.len() + 1).expect("manifest id");
    let roots = (1..manifest_id).collect();
    writer
        .add_manifest(manifest_id, &Manifest::new(roots))
        .expect("manifest");
    writer.finish(manifest_id).expect("file").inner
}
