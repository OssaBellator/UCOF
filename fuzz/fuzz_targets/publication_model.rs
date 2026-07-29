#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{
    CheckpointKind, PublicationLimits, PublicationModel, PublicationStage, SnapshotIdentity,
};

fuzz_target!(|data: &[u8]| {
    let identity = SnapshotIdentity::derive(data);
    let mut model = PublicationModel::new(
        identity,
        u64::from(data.first().copied().unwrap_or_default()),
        None,
        PublicationLimits {
            max_events: 128,
            max_complete_checkpoints: 16,
            max_progress_checkpoints: 32,
        },
    );

    for (index, byte) in data.iter().copied().take(128).enumerate() {
        match byte % 7 {
            0 => {
                let _ = model.advance(PublicationStage::Objects);
            }
            1 => {
                let _ = model.advance(PublicationStage::DirectoryLeaves);
            }
            2 => {
                let _ = model.advance(PublicationStage::DirectoryRoot);
            }
            3 => {
                let _ = model.advance(PublicationStage::SnapshotManifest);
            }
            4 => {
                let _ = model.advance(PublicationStage::Footer);
            }
            5 | 6 => {
                let checkpoint =
                    SnapshotIdentity::derive(&[byte, u8::try_from(index).unwrap_or(u8::MAX)]);
                let kind = if byte % 7 == 5 {
                    CheckpointKind::Complete
                } else {
                    CheckpointKind::Progress
                };
                let _ = model.checkpoint(
                    checkpoint,
                    u64::try_from(index).expect("bounded sequence"),
                    kind,
                );
            }
            _ => unreachable!(),
        }
    }

    let report = model.report();
    for checkpoint in report.progress_checkpoints {
        assert!(!checkpoint.independently_readable);
        assert!(!checkpoint.active_root_eligible);
    }
});
