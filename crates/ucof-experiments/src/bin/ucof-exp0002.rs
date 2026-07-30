use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use ucof_experiments::exp0002::{FileHeader, ValidationLimits};
use ucof_experiments::exp0002_rewrite::{
    repair_all_to_new_file, rewrite_selected_to_new_file, RewriteLimits, RewriteReport,
};
use ucof_experiments::exp0002_source::{
    lookup_authenticated_at, Exp0002SeekSource, Exp0002SourceLimits,
};
use ucof_experiments::{
    enumerate_previous_chain_at, scan_valid_prefixes_at, validate_strict_at,
    Exp0002SourceChainLimits, Exp0002SourceRecoveryLimits,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return Err(invalid_input("missing command").into());
    };

    match command.as_str() {
        "verify" => {
            let path = required(&mut args, "file")?;
            require_end(args)?;
            command_verify(Path::new(&path))
        }
        "roots" => {
            let path = required(&mut args, "file")?;
            require_end(args)?;
            command_roots(Path::new(&path))
        }
        "history" => {
            let path = required(&mut args, "file")?;
            require_end(args)?;
            command_history(Path::new(&path))
        }
        "lookup" => {
            let path = required(&mut args, "file")?;
            let object_id = parse_object_id(&required(&mut args, "object identifier")?)?;
            require_end(args)?;
            command_lookup(Path::new(&path), object_id)
        }
        "recover" => {
            let path = required(&mut args, "file")?;
            require_end(args)?;
            command_recover(Path::new(&path))
        }
        "repair-all" => {
            let input = required(&mut args, "input file")?;
            let output = required(&mut args, "output file")?;
            let header = parse_header(
                &required(&mut args, "16-byte file ID in hex")?,
                &required(&mut args, "16-byte creation nonce in hex")?,
            )?;
            require_end(args)?;
            command_repair_all(Path::new(&input), Path::new(&output), header)
        }
        "rewrite-selected" => {
            let input = required(&mut args, "input file")?;
            let output = required(&mut args, "output file")?;
            let header = parse_header(
                &required(&mut args, "16-byte file ID in hex")?,
                &required(&mut args, "16-byte creation nonce in hex")?,
            )?;
            let retained = parse_identifier_list(&required(&mut args, "retained IDs")?)?;
            let roots = parse_identifier_list(&required(&mut args, "root IDs")?)?;
            require_end(args)?;
            command_rewrite_selected(
                Path::new(&input),
                Path::new(&output),
                header,
                &retained,
                &roots,
            )
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        _ => {
            print_usage();
            Err(invalid_input(format!("unknown command: {command}")).into())
        }
    }
}

fn command_verify(path: &Path) -> Result<(), Box<dyn Error>> {
    let file = File::open(path)?;
    let mut source = Exp0002SeekSource::new(file);
    let report = validate_strict_at(&mut source, &Exp0002SourceLimits::default())?;
    println!("assurance: full strict exact-end validation");
    println!("sequence: {}", report.footer.sequence);
    println!("footer_offset: {}", report.footer_offset);
    println!("objects: {}", report.objects.len());
    println!("pages_verified: {}", report.pages_verified);
    println!("roots: {}", join_ids(&report.snapshot.roots));
    println!("snapshot_digest: {}", hex(&report.footer.snapshot_digest));
    println!("commit_digest: {}", hex(&report.footer.commit_digest));
    print_stats(report.stats);
    Ok(())
}

fn command_roots(path: &Path) -> Result<(), Box<dyn Error>> {
    let file = File::open(path)?;
    let mut source = Exp0002SeekSource::new(file);
    let report = validate_strict_at(&mut source, &Exp0002SourceLimits::default())?;
    println!("assurance: roots from fully validated active exact-end snapshot");
    println!("sequence: {}", report.footer.sequence);
    for root in &report.snapshot.roots {
        println!("root: {root}");
    }
    print_stats(report.stats);
    Ok(())
}

fn command_history(path: &Path) -> Result<(), Box<dyn Error>> {
    let file = File::open(path)?;
    let mut source = Exp0002SeekSource::new(file);
    let report = enumerate_previous_chain_at(&mut source, &Exp0002SourceChainLimits::default())?;
    println!("assurance: exact-end active commit and every linked ancestor validated as a strict prefix");
    println!("file_len: {}", report.file_len);
    println!("total_bytes_read: {}", report.total_bytes_read);
    println!("commits: {}", report.commits.len());
    for commit in report.commits {
        println!(
            "prefix={} footer={} sequence={} previous_footer={} roots={} parent_snapshot_digest={} snapshot_digest={} commit_digest={}",
            commit.prefix_len,
            commit.footer_offset,
            commit.sequence,
            commit.previous_footer_offset,
            join_ids(&commit.roots),
            hex(&commit.parent_snapshot_digest),
            hex(&commit.snapshot_digest),
            hex(&commit.commit_digest)
        );
    }
    Ok(())
}

fn command_lookup(path: &Path, object_id: u64) -> Result<(), Box<dyn Error>> {
    let file = File::open(path)?;
    let mut source = Exp0002SeekSource::new(file);
    match lookup_authenticated_at(&mut source, object_id, &Exp0002SourceLimits::default())? {
        Some(result) => {
            println!("assurance: authenticated active commit, one directory path, selected object");
            println!("object_id: {}", result.object_id);
            println!("kind: {}", result.kind);
            println!("sequence: {}", result.sequence);
            println!("record_offset: {}", result.record_offset);
            println!("record_len: {}", result.record_len);
            println!("payload_offset: {}", result.payload_offset);
            println!("payload_len: {}", result.payload_len);
            println!("logical_len: {}", result.logical_len);
            print_stats(result.stats);
        }
        None => {
            println!("assurance: authenticated absence in the active exact-end snapshot");
            println!("object_id: {object_id}");
        }
    }
    Ok(())
}

fn command_recover(path: &Path) -> Result<(), Box<dyn Error>> {
    let file = File::open(path)?;
    let mut source = Exp0002SeekSource::new(file);
    let report = scan_valid_prefixes_at(&mut source, &Exp0002SourceRecoveryLimits::default())?;
    println!("assurance: explicitly requested bounded recovery; each result is a strict prefix");
    println!("file_len: {}", report.file_len);
    println!("scan_start: {}", report.scan_start);
    println!("scan_bytes_read: {}", report.scan_bytes_read);
    println!("scan_read_operations: {}", report.scan_read_operations);
    println!("magic_matches: {}", report.magic_matches);
    println!("candidates_validated: {}", report.candidates_validated);
    println!(
        "total_candidate_bytes_read: {}",
        report.total_candidate_bytes_read
    );
    println!("verified_prefixes: {}", report.results.len());
    for candidate in report.results {
        println!(
            "prefix={} footer={} sequence={} previous_footer={} roots={} parent_snapshot_digest={} snapshot_digest={} commit_digest={}",
            candidate.prefix_len,
            candidate.footer_offset,
            candidate.sequence,
            candidate.previous_footer_offset,
            join_ids(&candidate.roots),
            hex(&candidate.parent_snapshot_digest),
            hex(&candidate.snapshot_digest),
            hex(&candidate.commit_digest)
        );
    }
    Ok(())
}

fn command_repair_all(
    input: &Path,
    output: &Path,
    header: FileHeader,
) -> Result<(), Box<dyn Error>> {
    let source = fs::read(input)?;
    let report = repair_all_to_new_file(&source, header, &RewriteLimits::default())?;
    write_new_output(output, &report.output)?;
    println!("assurance: verified-source repair to a new genesis file");
    print_rewrite_report(&report, output);
    Ok(())
}

fn command_rewrite_selected(
    input: &Path,
    output: &Path,
    header: FileHeader,
    retained: &[u64],
    roots: &[u64],
) -> Result<(), Box<dyn Error>> {
    let source = fs::read(input)?;
    let report =
        rewrite_selected_to_new_file(&source, header, retained, roots, &RewriteLimits::default())?;
    write_new_output(output, &report.output)?;
    println!("assurance: caller-selected verified rewrite to a new genesis file");
    println!("semantic_compaction_claim: false");
    print_rewrite_report(&report, output);
    Ok(())
}

fn print_rewrite_report(report: &RewriteReport, output: &Path) {
    println!("output: {}", output.display());
    println!("source_objects: {}", report.source_object_count);
    println!("output_objects: {}", report.output_object_count);
    println!("payload_bytes_copied: {}", report.payload_bytes_copied);
    println!(
        "source_snapshot_digest: {}",
        hex(&report.source_snapshot_digest)
    );
    println!(
        "output_snapshot_digest: {}",
        hex(&report.output_snapshot_digest)
    );
    println!(
        "source_commit_digest: {}",
        hex(&report.source_commit_digest)
    );
    println!(
        "output_commit_digest: {}",
        hex(&report.output_commit_digest)
    );
    println!(
        "snapshot_digest_preserved: {}",
        report.snapshot_digest_preserved
    );
    println!(
        "commit_digest_preserved: {}",
        report.commit_digest_preserved
    );
    println!(
        "byte_scoped_signatures_preserved: {}",
        report.byte_scoped_signatures_preserved
    );
}

fn write_new_output(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(bytes)?;
    output.sync_all()
}

fn print_stats(stats: ucof_experiments::exp0002_source::Exp0002SourceStats) {
    println!("read_operations: {}", stats.read_operations);
    println!("bytes_read: {}", stats.bytes_read);
    println!("largest_request: {}", stats.largest_request);
    println!("bytes_hashed: {}", stats.bytes_hashed);
    println!("pages_read: {}", stats.pages_read);
}

fn parse_header(file_id: &str, nonce: &str) -> Result<FileHeader, Box<dyn Error>> {
    Ok(FileHeader {
        file_id: parse_fixed_hex(file_id, "file ID")?,
        creation_nonce: parse_fixed_hex(nonce, "creation nonce")?,
    })
}

fn parse_fixed_hex<const N: usize>(value: &str, name: &str) -> Result<[u8; N], Box<dyn Error>> {
    if value.len() != N * 2 {
        return Err(invalid_input(format!(
            "{name} must contain exactly {} hexadecimal characters",
            N * 2
        ))
        .into());
    }
    let mut output = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or_else(|| invalid_input(format!("invalid {name}")))?;
        let low = hex_digit(pair[1]).ok_or_else(|| invalid_input(format!("invalid {name}")))?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_identifier_list(value: &str) -> Result<Vec<u64>, Box<dyn Error>> {
    let mut values = Vec::new();
    for item in value.split(',') {
        if item.is_empty() {
            return Err(invalid_input("identifier lists cannot contain empty items").into());
        }
        values.push(parse_object_id(item)?);
    }
    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        return Err(invalid_input("identifier list cannot be empty").into());
    }
    Ok(values)
}

fn parse_object_id(value: &str) -> Result<u64, Box<dyn Error>> {
    let parsed: u64 = value.parse()?;
    if parsed == 0 {
        return Err(invalid_input("object identifiers must be non-zero").into());
    }
    Ok(parsed)
}

fn required(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, Box<dyn Error>> {
    args.next()
        .ok_or_else(|| invalid_input(format!("missing {name}")).into())
}

fn require_end(mut args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    if let Some(extra) = args.next() {
        Err(invalid_input(format!("unexpected argument: {extra}")).into())
    } else {
        Ok(())
    }
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn join_ids(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn print_usage() {
    eprintln!(
        "Usage:\n  \
         ucof-exp0002 verify <file>\n  \
         ucof-exp0002 roots <file>\n  \
         ucof-exp0002 history <file>\n  \
         ucof-exp0002 lookup <file> <object-id>\n  \
         ucof-exp0002 recover <file>\n  \
         ucof-exp0002 repair-all <input> <output> <file-id-hex> <nonce-hex>\n  \
         ucof-exp0002 rewrite-selected <input> <output> <file-id-hex> <nonce-hex> <retained-csv> <roots-csv>\n\n\
         verify never invokes recovery; history validates the exact linked chain; \
         recover reports only strictly validated prefixes; rewrite-selected is caller-directed \
         and does not claim semantic compaction."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_hex_parser_requires_exact_length() {
        assert_eq!(parse_fixed_hex::<2>("00ff", "test").expect("hex"), [0, 255]);
        assert!(parse_fixed_hex::<2>("00", "test").is_err());
        assert!(parse_fixed_hex::<2>("00zz", "test").is_err());
    }

    #[test]
    fn identifier_lists_are_canonicalized() {
        assert_eq!(
            parse_identifier_list("3,1,3,2").expect("list"),
            vec![1, 2, 3]
        );
        assert!(parse_identifier_list("0").is_err());
        assert!(parse_identifier_list("1,,2").is_err());
    }
}
