#[path = "../src/bounded_source_descriptor.rs"]
mod bounded_source_descriptor;
mod bounded_source_descriptor_parse {
    include!("../src/bounded_source_descriptor_parse.rs");
}
mod bounded_source_descriptor_visit {
    include!("../src/bounded_source_descriptor_visit.rs");
}
#[path = "../src/bounded_spill_fallible.rs"]
mod bounded_spill_fallible;
#[path = "../src/bounded_spill_sort.rs"]
mod bounded_spill_sort;

use bounded_source_descriptor::{BoundedSourceDescriptor, BOUNDED_SOURCE_DESCRIPTOR_BYTES};
use bounded_source_descriptor_visit::visit_bounded_source_descriptors;
use bounded_spill_sort::BoundedSpillSortLimits;

#[test]
fn visitor_sorts_fixed_source_descriptors() {
    let directory = std::env::temp_dir().join(format!("ucof-descriptor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).expect("create directory");
    let limits = BoundedSpillSortLimits {
        record_bytes: BOUNDED_SOURCE_DESCRIPTOR_BYTES,
        run_records: 2,
        max_records: 8,
        max_initial_runs: 4,
        max_open_inputs: 2,
        max_merge_passes: 4,
        max_live_spill_bytes: 64 * 1024,
        max_spill_bytes_written: 256 * 1024,
        max_merge_bytes_read: 256 * 1024,
        max_merge_bytes_written: 256 * 1024,
    };
    let input = [3u64, 1, 2].into_iter().map(|object_id| {
        Ok::<_, &'static str>(BoundedSourceDescriptor {
            object_id,
            source_index: object_id,
            kind: 1,
            logical_len: object_id * 10,
            strong_version: [u8::try_from(object_id).expect("version"); 32],
        })
    });
    let mut visited = Vec::new();
    visit_bounded_source_descriptors(&directory, input, limits, |descriptor| {
        visited.push(descriptor.object_id);
        Ok::<_, &'static str>(())
    })
    .expect("visit descriptors");
    assert_eq!(visited, vec![1, 2, 3]);
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}
