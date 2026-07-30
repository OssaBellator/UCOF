#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{
    CandidateStatus, CheckpointKind, EnumerationLimits, RootEnumerationReport, SnapshotCandidate,
    SnapshotIdentity,
};

fuzz_target!(|data: &[u8]| {
    let count = data.len().min(32);
    let identities: Vec<_> = (0..count)
        .map(|index| SnapshotIdentity::derive(&[u8::try_from(index).expect("bounded index")]))
        .collect();
    let mut candidates = Vec::with_capacity(count);
    for index in 0..count {
        let byte = data[index];
        let parent = if index == 0 || byte & 1 == 0 {
            None
        } else {
            Some(identities[usize::from(byte) % count])
        };
        let status = match (byte >> 1) % 5 {
            0 => CandidateStatus::Verified,
            1 => CandidateStatus::IntegrityFailed,
            2 => CandidateStatus::UnsupportedRequiredCapability,
            3 => CandidateStatus::Truncated,
            _ => CandidateStatus::Invalid,
        };
        candidates.push(SnapshotCandidate {
            identity: identities[index],
            sequence: u64::from(byte >> 3),
            parent,
            footer_offset: u64::try_from(index).expect("bounded index") * 128,
            exact_end: byte & 0x80 != 0,
            checkpoint: if byte & 0x40 == 0 {
                CheckpointKind::Complete
            } else {
                CheckpointKind::Progress
            },
            status,
        });
    }

    let _ = RootEnumerationReport::enumerate(
        &candidates,
        EnumerationLimits {
            max_candidates: 32,
            max_parent_depth: 32,
            max_results: 32,
        },
    );
});
