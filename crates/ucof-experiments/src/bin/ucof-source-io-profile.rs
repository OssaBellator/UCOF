use std::error::Error;

use ucof_experiments::immutable_successor::{
    build_genesis, lookup_at, rewrite_source_selected, validate_source_at, ImmutableLimits,
    ImmutableLookupResult, ImmutableObjectInput, ImmutableSliceSource, ImmutableSourceLimits,
    ImmutableSourceStats,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProfileRow {
    objects: usize,
    file_bytes: usize,
    root_level: u8,
    strict: ImmutableSourceStats,
    lookup: ImmutableSourceStats,
    selected_rewrite: ImmutableSourceStats,
}

fn limits() -> ImmutableSourceLimits {
    ImmutableSourceLimits {
        format: ImmutableLimits {
            max_file_bytes: 64 * 1024 * 1024,
            max_objects: 2_000,
            max_pages: 4_096,
            max_depth: 8,
            max_allocation_bytes: 4 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
            ..ImmutableLimits::default()
        },
        max_total_bytes_read: 512 * 1024 * 1024,
        max_read_operations: 5_000_000,
        max_read_request_bytes: 1_024,
        hash_block_bytes: 1_024,
    }
}

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("bounded object count"))
        .map(|object_id| {
            let seed = u8::try_from(object_id % 251).expect("seed");
            ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; 64])
        })
        .collect()
}

fn profile(count: usize) -> Result<ProfileRow, Box<dyn Error>> {
    let limits = limits();
    let bytes = build_genesis(&objects(count), limits.format)?;

    let mut strict_source = ImmutableSliceSource::new(&bytes);
    let strict = validate_source_at(&mut strict_source, limits)?;

    let target = u64::try_from(count / 2 + 1)?;
    let mut lookup_source = ImmutableSliceSource::new(&bytes);
    let lookup = lookup_at(&mut lookup_source, target, limits)?;
    if !matches!(
        lookup.result,
        ImmutableLookupResult::Found { object_id, .. } if object_id == target
    ) {
        return Err("profile lookup did not find its target".into());
    }

    let last = u64::try_from(count)?;
    let selected = if count == 1 { vec![1] } else { vec![1, last] };
    let mut rewrite_source = ImmutableSliceSource::new(&bytes);
    let rewrite = rewrite_source_selected(&mut rewrite_source, &selected, limits)?;
    if rewrite.rewrite.output.object_count != selected.len() {
        return Err("profile rewrite selected the wrong object count".into());
    }

    if lookup.stats.bytes_read > strict.stats.bytes_read {
        return Err("path lookup exceeded complete strict validation reads".into());
    }
    if rewrite.stats.bytes_read < strict.stats.bytes_read {
        return Err("source rewrite did not include strict validation work".into());
    }
    for stats in [strict.stats, lookup.stats, rewrite.stats] {
        if stats.largest_allocation > limits.format.max_allocation_bytes
            || stats.largest_allocation >= bytes.len()
        {
            return Err("profile allocation boundary was not preserved".into());
        }
    }

    Ok(ProfileRow {
        objects: count,
        file_bytes: bytes.len(),
        root_level: strict.report.root_level,
        strict: strict.stats,
        lookup: lookup.stats,
        selected_rewrite: rewrite.stats,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "objects,file_bytes,root_level,strict_reads,strict_bytes,strict_hashed,lookup_reads,lookup_bytes,lookup_hashed,rewrite_reads,rewrite_bytes,rewrite_hashed,largest_allocation"
    );
    for count in [1_usize, 185, 400, 1_000] {
        let row = profile(count)?;
        println!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            row.objects,
            row.file_bytes,
            row.root_level,
            row.strict.read_operations,
            row.strict.bytes_read,
            row.strict.bytes_hashed,
            row.lookup.read_operations,
            row.lookup.bytes_read,
            row.lookup.bytes_hashed,
            row.selected_rewrite.read_operations,
            row.selected_rewrite.bytes_read,
            row.selected_rewrite.bytes_hashed,
            row.strict
                .largest_allocation
                .max(row.lookup.largest_allocation)
                .max(row.selected_rewrite.largest_allocation),
        );
    }
    Ok(())
}
