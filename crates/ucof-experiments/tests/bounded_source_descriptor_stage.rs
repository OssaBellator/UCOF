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
use bounded_source_descriptor_stage::{
    prepare_bounded_source_descriptors, BoundedSourceStageError, BoundedSourceStageVisitError,
};
use bounded_spill_sort::{BoundedSpillSortError, BoundedSpillSortLimits};

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

fn descriptor(object_id: u64) -> BoundedSourceDescriptor {
    BoundedSourceDescriptor {
        object_id,
        source_index: object_id + 100,
        kind: 1,
        logical_len: object_id * 10,
        strong_version: [u8::try_from(object_id).expect("version"); 32],
    }
}

fn directory(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ucof-prepared-descriptors-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir(&path).expect("create directory");
    path
}

#[test]
fn prepared_stage_releases_input_before_sorted_visit() {
    let directory = directory("success");
    let input = [4u64, 1, 3, 2]
        .into_iter()
        .map(|object_id| Ok::<_, &'static str>(descriptor(object_id)));
    let stage = prepare_bounded_source_descriptors(&directory, input, limits())
        .expect("prepare descriptors");
    assert_eq!(stage.records(), 4);
    assert_eq!(stage.bytes(), 4 * BOUNDED_SOURCE_DESCRIPTOR_BYTES as u64);
    assert_eq!(stage.report().output_records, 4);
    let mut visited = Vec::new();
    stage
        .visit(|descriptor| {
            visited.push(descriptor.object_id);
            Ok::<_, &'static str>(())
        })
        .expect("visit stage");
    assert_eq!(visited, vec![1, 2, 3, 4]);
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    drop(stage);
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}

#[test]
fn preparation_failures_leave_no_stage_file() {
    let directory = directory("failures");
    let input = vec![
        Ok(descriptor(2)),
        Ok(descriptor(1)),
        Err("metadata read"),
    ];
    let error = prepare_bounded_source_descriptors(&directory, input, limits())
        .expect_err("input failure");
    assert!(matches!(error, BoundedSourceStageError::Input("metadata read")));
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());

    let duplicate = [1u64, 3, 2, 4, 2]
        .into_iter()
        .map(|object_id| Ok::<_, &'static str>(descriptor(object_id)));
    let error = prepare_bounded_source_descriptors(&directory, duplicate, limits())
        .expect_err("duplicate failure");
    assert!(matches!(
        error,
        BoundedSourceStageError::Sort(BoundedSpillSortError::DuplicateKey(2))
    ));
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}

#[test]
fn visitor_failure_preserves_typed_error_and_stage_for_retry() {
    let directory = directory("visitor-failure");
    let input = [2u64, 1]
        .into_iter()
        .map(|object_id| Ok::<_, &'static str>(descriptor(object_id)));
    let stage = prepare_bounded_source_descriptors(&directory, input, limits())
        .expect("prepare descriptors");
    let error = stage
        .visit(|descriptor| {
            if descriptor.object_id == 2 {
                Err("stop")
            } else {
                Ok(())
            }
        })
        .expect_err("visitor failure");
    assert!(matches!(error, BoundedSourceStageVisitError::Visit("stop")));
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    drop(stage);
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(&directory).expect("remove directory");
}
