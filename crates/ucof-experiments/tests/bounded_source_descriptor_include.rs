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
use bounded_source_descriptor_visit::{
    visit_bounded_source_descriptors, BoundedSourceDescriptorError,
};
use bounded_spill_sort::{BoundedSpillSortError, BoundedSpillSortLimits};

fn limits() -> BoundedSpillSortLimits {
    BoundedSpillSortLimits {
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
    }
}

fn descriptor(object_id: u64) -> BoundedSourceDescriptor {
    BoundedSourceDescriptor {
        object_id,
        source_index: object_id,
        kind: 1,
        logical_len: object_id * 10,
        strong_version: [u8::try_from(object_id).expect("version"); 32],
    }
}

fn directory(label: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "ucof-source-descriptor-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).expect("create directory");
    directory
}

#[test]
fn visitor_sorts_fixed_source_descriptors() {
    let directory = directory("success");
    let input = [3u64, 1, 2]
        .into_iter()
        .map(|object_id| Ok::<_, &'static str>(descriptor(object_id)));
    let mut visited = Vec::new();
    visit_bounded_source_descriptors(&directory, input, limits(), |descriptor| {
        visited.push(descriptor.object_id);
        Ok::<_, &'static str>(())
    })
    .expect("visit descriptors");
    assert_eq!(visited, vec![1, 2, 3]);
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}

#[test]
fn input_failure_after_completed_run_invokes_no_visitor() {
    let directory = directory("input-failure");
    let input = vec![
        Ok(descriptor(2)),
        Ok(descriptor(1)),
        Err("metadata read"),
        Ok(descriptor(3)),
    ];
    let mut visited = 0usize;
    let error = visit_bounded_source_descriptors(&directory, input, limits(), |_| {
        visited += 1;
        Ok::<_, &'static str>(())
    })
    .expect_err("input failure");
    assert!(matches!(
        error,
        BoundedSourceDescriptorError::Input("metadata read")
    ));
    assert_eq!(visited, 0);
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}

#[test]
fn duplicate_across_runs_invokes_no_visitor() {
    let directory = directory("duplicate");
    let input = [1u64, 3, 2, 4, 2]
        .into_iter()
        .map(|object_id| Ok::<_, &'static str>(descriptor(object_id)));
    let mut visited = 0usize;
    let error = visit_bounded_source_descriptors(&directory, input, limits(), |_| {
        visited += 1;
        Ok::<_, &'static str>(())
    })
    .expect_err("duplicate failure");
    assert!(matches!(
        error,
        BoundedSourceDescriptorError::Sort(BoundedSpillSortError::DuplicateKey(2))
    ));
    assert_eq!(visited, 0);
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}

#[test]
fn invalid_record_configuration_preserves_label() {
    let directory = directory("invalid-config");
    let mut invalid_limits = limits();
    invalid_limits.record_bytes -= 1;
    let error = visit_bounded_source_descriptors(
        &directory,
        std::iter::empty::<Result<BoundedSourceDescriptor, &'static str>>(),
        invalid_limits,
        |_| Ok::<_, &'static str>(()),
    )
    .expect_err("invalid configuration");
    assert!(matches!(
        error,
        BoundedSourceDescriptorError::Invalid("spill record byte configuration")
    ));
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}
