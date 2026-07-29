use std::env;
use std::fs::File;
use ucof_core::{IntegrityStatus, Limits, SequentialReader, StreamEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: cargo run -p ucof-core --example sequential_verify -- <file>")?;
    let file = File::open(path)?;
    let mut reader = SequentialReader::new(file, Limits::default());

    while let Some(event) = reader.next_event()? {
        match event {
            StreamEvent::RecordStart(record) => println!(
                "record id={} kind={:?} stored={}",
                record.object_id, record.kind, record.stored_len
            ),
            StreamEvent::PayloadChunk {
                object_id,
                bytes,
                remaining,
                ..
            } => println!(
                "payload id={} chunk={} remaining={}",
                object_id,
                bytes.len(),
                remaining
            ),
            StreamEvent::Commit(commit) => {
                assert_eq!(commit.integrity, IntegrityStatus::Verified);
                println!(
                    "verified manifest={} records={} hashed={}",
                    commit.manifest_id, commit.record_count, commit.stats.bytes_hashed
                );
            }
            _ => {}
        }
    }
    Ok(())
}
