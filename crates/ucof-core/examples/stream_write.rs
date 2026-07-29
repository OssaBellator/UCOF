use std::env;
use std::fs::File;
use ucof_core::{Manifest, StreamingWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: cargo run -p ucof-core --example stream_write -- <file>")?;
    let output = File::create(path)?;
    let mut writer = StreamingWriter::with_default_limits(output)?;
    writer.add_opaque(1, b"hello from UCOF")?;
    writer.add_manifest(2, &Manifest::new(vec![1]))?;
    let finished = writer.finish(2)?;
    println!("wrote {} bytes", finished.bytes_written);
    Ok(())
}
