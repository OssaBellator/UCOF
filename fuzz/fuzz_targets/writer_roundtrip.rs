#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_core::{Limits, Manifest, StreamingWriter, ValidatedFile, Writer};

fuzz_target!(|data: &[u8]| {
    let object_count = usize::from(data.first().copied().unwrap_or(0) % 8) + 1;
    let body = data.get(1..).unwrap_or_default();
    let chunk = body.len().div_ceil(object_count);

    let mut in_memory = Writer::new();
    let mut streaming = StreamingWriter::with_default_limits(Vec::new()).expect("writer");
    for index in 0..object_count {
        let start = index.saturating_mul(chunk).min(body.len());
        let end = start.saturating_add(chunk).min(body.len());
        let payload = &body[start..end];
        let id = u64::try_from(index + 1).expect("bounded id");
        in_memory.add_opaque(id, payload).expect("in-memory object");
        streaming.add_opaque(id, payload).expect("streaming object");
    }

    let manifest_id = u64::try_from(object_count + 1).expect("bounded manifest id");
    let roots = (1..manifest_id).collect();
    let manifest = Manifest::new(roots);
    in_memory
        .add_manifest(manifest_id, &manifest)
        .expect("in-memory manifest");
    streaming
        .add_manifest(manifest_id, &manifest)
        .expect("streaming manifest");

    let expected = in_memory.finish(manifest_id).expect("in-memory finish");
    let actual = streaming.finish(manifest_id).expect("streaming finish").inner;
    assert_eq!(actual, expected);
    ValidatedFile::parse(&actual, &Limits::default()).expect("round-trip validation");
});
