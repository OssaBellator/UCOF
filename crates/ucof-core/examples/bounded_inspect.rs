use std::env;
use std::fs::File;
use ucof_core::{IntegrityStatus, Limits, MetadataInspector, SeekSource};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: cargo run -p ucof-core --example bounded_inspect -- <file>")?;
    let file = File::open(path)?;
    let mut source = SeekSource::new(file);
    let report = MetadataInspector::new(Limits::default()).inspect(&mut source)?;

    assert_eq!(report.integrity, IntegrityStatus::NotChecked);
    println!(
        "epoch={} manifest={} objects={} bytes_read={}",
        report.epoch,
        report.manifest_id,
        report.entries.len(),
        report.stats.bytes_read
    );
    for entry in report.entries {
        println!(
            "id={} kind={:?} offset={} stored={}",
            entry.id, entry.kind, entry.offset, entry.stored_len
        );
    }
    Ok(())
}
