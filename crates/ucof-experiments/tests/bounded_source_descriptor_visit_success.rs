#[path = "../src/bounded_source_descriptor.rs"]
mod bounded_source_descriptor;
#[path = "../src/bounded_source_descriptor_parse.rs"]
mod bounded_source_descriptor_parse;
#[path = "../src/bounded_source_descriptor_visit.rs"]
mod bounded_source_descriptor_visit;
#[path = "../src/bounded_spill_fallible.rs"]
mod bounded_spill_fallible;
#[path = "../src/bounded_spill_sort.rs"]
mod bounded_spill_sort;

use bounded_source_descriptor::{BoundedSourceDescriptor, BOUNDED_SOURCE_DESCRIPTOR_BYTES};
use bounded_source_descriptor_visit::visit_bounded_source_descriptors;
use bounded_spill_sort::BoundedSpillSortLimits;
use std::fs;

fn limits() -> BoundedSpillSortLimits {
    BoundedSpillSortLimits {
        record_bytes: BOUNDED_SOURCE_DESCRIPTOR_BYTES,
        run_records: 2,
        max_records: 16,
        max_initial_runs: 8,
        max_open_inputs: 2,
        max_merge_passes: 8,
        max_live_spill_bytes: 64 * 1024,
        max_spill_bytes_written: 256 * 1024,
        max_merge_bytes_read: 256 * 1024,
        max_merge_bytes_written: 256 * 1024,
    }
}

#[test]
fn sorted_descriptor_visitor_is_run_independent() {
    let directory = std::env::temp_dir().join(format!(
        "ucof-source-descriptor-success-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).expect("create test directory");
    let input = [5u64, 1, 4, 2, 3].into_iter().map(|object_id| {
        Ok::<_, &'static str>(BoundedSourceDescriptor {
            object_id,
            source_index: object_id + 10,
            kind: 1,
            logical_len: object_id * 100,
            strong_version: [u8::try_from(object_id).expect("version"); 32],
        })
    });
    let mut visited = Vec::new();
    let report = visit_bounded_source_descriptors(&directory, input, limits(), |descriptor| {
        visited.push(descriptor.object_id);
        Ok::<_, &'static str>(())
    })
    .expect("sort descriptors");
    assert_eq!(visited, vec![1, 2, 3, 4, 5]);
    assert_eq!(report.output_records, 5);
    assert!(fs::read_dir(&directory).unwrap().next().is_none());
    fs::remove_dir(&directory).expect("remove test directory");
}
