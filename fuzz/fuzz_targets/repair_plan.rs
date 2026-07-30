#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{
    CandidateStatus, CheckpointKind, CompactionLimits, ObjectGraph, ObjectLocator, RepairLimits,
    RepairPlan, SnapshotCandidate, SnapshotIdentity,
};

fuzz_target!(|data: &[u8]| {
    let node_count = data.len().clamp(1, 32);
    let mut graph = ObjectGraph::new();
    let mut locators = Vec::new();
    let mut offset = 0_u64;

    for index in 0..node_count {
        let byte = data.get(index).copied().unwrap_or_default();
        let object_id = u64::try_from(index + 1).expect("bounded object id");
        let dependency = if index > 0 && byte & 1 != 0 {
            vec![u64::try_from(usize::from(byte) % index + 1).expect("bounded dependency")]
        } else {
            Vec::new()
        };
        graph
            .add_object(object_id, dependency)
            .expect("unique generated object");
        let length = u64::from(byte) + 1;
        locators.push(ObjectLocator {
            object_id,
            kind: 1,
            offset,
            stored_len: length,
            logical_len: length,
        });
        offset = offset.saturating_add(length + u64::from(byte & 0x03));
    }

    if data.first().is_some_and(|byte| byte & 0x80 != 0) && locators.len() > 1 {
        locators[1].offset = locators[0].offset;
    }

    let first = data.first().copied().unwrap_or_default();
    let snapshot = SnapshotCandidate {
        identity: SnapshotIdentity::derive(data),
        sequence: u64::from(first),
        parent: None,
        footer_offset: offset,
        exact_end: first & 0x20 != 0,
        checkpoint: if first & 0x40 == 0 {
            CheckpointKind::Complete
        } else {
            CheckpointKind::Progress
        },
        status: if first & 0x10 == 0 {
            CandidateStatus::Verified
        } else {
            CandidateStatus::IntegrityFailed
        },
    };

    let _ = RepairPlan::build(
        snapshot,
        &[1],
        &graph,
        locators,
        RepairLimits {
            compaction: CompactionLimits {
                max_nodes: 32,
                max_edges: 64,
                max_depth: 32,
            },
            max_copy_ranges: 32,
            max_total_copy_bytes: 8192,
        },
    );
});
