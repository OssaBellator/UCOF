#[path = "../src/bounded_source_descriptor.rs"]
mod bounded_source_descriptor;
mod bounded_source_descriptor_parse {
    include!("../src/bounded_source_descriptor_parse.rs");
}
mod bounded_source_descriptor_stage {
    include!("../src/bounded_source_descriptor_stage.rs");
}
#[path = "../src/bounded_spill_fallible.rs"]
mod bounded_spill_fallible;
#[path = "../src/bounded_spill_sort.rs"]
mod bounded_spill_sort;

use bounded_source_descriptor::{BoundedSourceDescriptor, BOUNDED_SOURCE_DESCRIPTOR_BYTES};
use bounded_source_descriptor_stage::prepare_bounded_source_descriptors;
use bounded_spill_sort::BoundedSpillSortLimits;
use sha2::{Digest, Sha256};
use ucof_experiments::immutable_successor::ImmutableStreamingPayloadSource;

#[derive(Clone, Debug)]
struct MemorySource {
    object_id: u64,
    kind: u16,
    bytes: Vec<u8>,
    version: [u8; 32],
    reads: usize,
}

impl MemorySource {
    fn new(object_id: u64) -> Self {
        Self {
            object_id,
            kind: 1,
            bytes: vec![u8::try_from(object_id).expect("payload seed"); object_id as usize * 3],
            version: [u8::try_from(object_id).expect("version seed"); 32],
            reads: 0,
        }
    }
}

impl ImmutableStreamingPayloadSource for MemorySource {
    fn object_id(&self) -> u64 {
        self.object_id
    }

    fn kind(&self) -> u16 {
        self.kind
    }

    fn logical_len(&self) -> u64 {
        u64::try_from(self.bytes.len()).expect("payload length")
    }

    fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
        Ok(self.version)
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), &'static str> {
        let start = usize::try_from(offset).map_err(|_| "offset")?;
        let end = start.checked_add(buffer.len()).ok_or("range")?;
        buffer.copy_from_slice(self.bytes.get(start..end).ok_or("range")?);
        self.reads += 1;
        Ok(())
    }
}

fn limits() -> BoundedSpillSortLimits {
    BoundedSpillSortLimits {
        record_bytes: BOUNDED_SOURCE_DESCRIPTOR_BYTES,
        run_records: 2,
        max_records: 16,
        max_initial_runs: 8,
        max_open_inputs: 2,
        max_merge_passes: 8,
        max_live_spill_bytes: 128 * 1024,
        max_spill_bytes_written: 512 * 1024,
        max_merge_bytes_read: 512 * 1024,
        max_merge_bytes_written: 512 * 1024,
    }
}

#[test]
fn prepared_descriptors_release_source_borrow_before_payload_streaming() {
    let directory = std::env::temp_dir().join(format!("ucof-source-borrow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).expect("create directory");
    let mut sources = vec![
        MemorySource::new(5),
        MemorySource::new(1),
        MemorySource::new(4),
        MemorySource::new(2),
        MemorySource::new(3),
    ];

    let descriptors = sources.iter_mut().enumerate().map(|(index, source)| {
        Ok::<_, &'static str>(BoundedSourceDescriptor {
            object_id: source.object_id(),
            source_index: u64::try_from(index).expect("source index"),
            kind: source.kind(),
            logical_len: source.logical_len(),
            strong_version: source.strong_version()?,
        })
    });
    let stage = prepare_bounded_source_descriptors(&directory, descriptors, limits())
        .expect("prepare source descriptors");

    let mut actual = Sha256::new();
    stage
        .visit(|descriptor| {
            let index = usize::try_from(descriptor.source_index).map_err(|_| "source index")?;
            let source = sources.get_mut(index).ok_or("source index")?;
            if source.object_id() != descriptor.object_id
                || source.kind() != descriptor.kind
                || source.logical_len() != descriptor.logical_len
                || source.strong_version()? != descriptor.strong_version
            {
                return Err("descriptor changed");
            }
            let mut offset = 0u64;
            let mut buffer = [0u8; 7];
            while offset < descriptor.logical_len {
                let remaining =
                    usize::try_from(descriptor.logical_len - offset).map_err(|_| "remaining")?;
                let take = remaining.min(buffer.len());
                source.read_exact_at(offset, &mut buffer[..take])?;
                actual.update(&buffer[..take]);
                offset += u64::try_from(take).map_err(|_| "take")?;
            }
            if source.strong_version()? != descriptor.strong_version {
                return Err("version changed");
            }
            Ok::<_, &'static str>(())
        })
        .expect("stream sorted source payloads");

    let mut expected = Sha256::new();
    for object_id in 1u64..=5 {
        expected.update(vec![
            u8::try_from(object_id).expect("payload seed");
            object_id as usize * 3
        ]);
    }
    assert_eq!(actual.finalize()[..], expected.finalize()[..]);
    assert!(sources.iter().all(|source| source.reads > 0));
    drop(stage);
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}
