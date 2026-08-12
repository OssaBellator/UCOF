#[path = "../src/cow_reuse.rs"]
mod cow_reuse;

use cow_reuse::{Directory, Entry, Limits};

#[test]
fn persistent_updates_preserve_old_snapshot_lookup() {
    let original = Directory::build(
        (1_u64..=10_000)
            .map(|key| Entry { key, revision: 0 })
            .collect(),
        185,
        255,
    )
    .expect("directory");
    let updated = original
        .upsert(
            Entry {
                key: 5_000,
                revision: 1,
            },
            Limits::default(),
        )
        .expect("update");

    assert_eq!(
        original.lookup(5_000).expect("old lookup"),
        Some(Entry {
            key: 5_000,
            revision: 0
        })
    );
    assert_eq!(
        updated.directory.lookup(5_000).expect("new lookup"),
        Some(Entry {
            key: 5_000,
            revision: 1
        })
    );
    assert!(updated.reused_pages > 0);
}
