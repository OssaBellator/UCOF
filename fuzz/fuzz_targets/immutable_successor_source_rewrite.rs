#![no_main]

use std::collections::BTreeSet;

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    build_genesis, rewrite_selected, rewrite_source_all, rewrite_source_selected, ImmutableLimits,
    ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableSourceLimits,
};

#[derive(Debug)]
struct BoundedSource {
    bytes: Vec<u8>,
    largest_request: usize,
}

impl BoundedSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            largest_request: 0,
        }
    }
}

impl ImmutableReadAt for BoundedSource {
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
        self.largest_request = self.largest_request.max(buffer.len());
        Ok(())
    }
}

fn source_limits() -> ImmutableSourceLimits {
    ImmutableSourceLimits {
        format: ImmutableLimits {
            max_file_bytes: 2 * 1024 * 1024,
            max_objects: 32,
            max_pages: 64,
            max_depth: 4,
            max_allocation_bytes: 2 * 1024 * 1024,
            max_output_bytes: 2 * 1024 * 1024,
            ..ImmutableLimits::default()
        },
        max_total_bytes_read: 4 * 1024 * 1024,
        max_read_operations: 100_000,
        max_read_request_bytes: 256,
        hash_block_bytes: 256,
        ..ImmutableSourceLimits::default()
    }
}

fuzz_target!(|data: &[u8]| {
    let count = data.first().map_or(1_usize, |byte| 1 + usize::from(*byte % 8));
    let mut objects = Vec::with_capacity(count);
    for index in 0..count {
        let seed = data.get(index + 1).copied().unwrap_or(index as u8);
        objects.push(ImmutableObjectInput::new(
            u64::try_from(index + 1).expect("small object identifier"),
            u16::from(1 + seed % 31),
            vec![seed, seed.rotate_left(1), seed.rotate_left(2), seed.rotate_left(3)],
        ));
    }

    let limits = source_limits();
    let genesis = build_genesis(&objects, limits.format).expect("bounded genesis");
    let mut selected = BTreeSet::new();
    for byte in data.iter().skip(count + 1).take(8) {
        selected.insert(1 + u64::from(*byte) % u64::try_from(count).expect("small count"));
    }
    if selected.is_empty() {
        selected.insert(1);
    }
    let selected: Vec<u64> = selected.into_iter().collect();

    let expected = rewrite_selected(&genesis, &selected, limits.format)
        .expect("bounded slice selected rewrite");
    let mut source = BoundedSource::new(genesis.clone());
    let actual = rewrite_source_selected(&mut source, &selected, limits)
        .expect("bounded source selected rewrite");
    assert_eq!(actual.rewrite, expected);
    assert!(source.largest_request <= limits.max_read_request_bytes);

    let mutation_seed = data.last().copied().unwrap_or(0);
    let mut corrupted = genesis;
    let mutation_offset = usize::from(mutation_seed) % corrupted.len();
    corrupted[mutation_offset] ^= 1;
    let mut source = BoundedSource::new(corrupted);
    assert!(rewrite_source_all(&mut source, limits).is_err());
});
