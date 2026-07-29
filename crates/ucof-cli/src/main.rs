use std::env;
use std::error::Error;
use std::fs::File;
use std::path::Path;
use std::process::ExitCode;
use ucof_core::{
    DiagnosticStatus, DiagnosticValidator, Limits, Manifest, MetadataInspector, PrefixSalvager,
    SeekSource, SourceValidator, StreamingWriter,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ucof: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args();
    let _program = args.next();
    let command = args.next().ok_or("missing command")?;
    match command.as_str() {
        "inspect" => {
            let path = one_path(args, "inspect")?;
            let file = File::open(path)?;
            let mut source = SeekSource::new(file);
            let report = MetadataInspector::new(Limits::default()).inspect(&mut source)?;
            println!(
                "UCOF-EXP-0001 metadata: manifest {}, {} objects, integrity not checked",
                report.manifest_id,
                report.entries.len()
            );
            for entry in report.entries {
                println!(
                    "id={} kind={:?} offset={} stored={} logical={}",
                    entry.id, entry.kind, entry.offset, entry.stored_len, entry.logical_len
                );
            }
            println!(
                "reads={} bytes_read={}",
                report.stats.read_operations, report.stats.bytes_read
            );
        }
        "verify" => {
            let path = one_path(args, "verify")?;
            let file = File::open(path)?;
            let mut source = SeekSource::new(file);
            let report = SourceValidator::new(Limits::default()).validate(&mut source)?;
            println!(
                "verified UCOF-EXP-0001: {} objects, manifest {}, hashed {} bytes",
                report.entries.len(),
                report.manifest_id,
                report.stats.bytes_hashed
            );
        }
        "diagnose" => {
            let path = one_path(args, "diagnose")?;
            let file = File::open(path)?;
            let mut source = SeekSource::new(file);
            let report = DiagnosticValidator::new(Limits::default()).diagnose(&mut source)?;
            println!("diagnostic status: {:?}", report.status);
            for diagnostic in &report.diagnostics {
                println!(
                    "stage={:?} category={:?} offset={:?}: {}",
                    diagnostic.stage,
                    diagnostic.category,
                    diagnostic.offset,
                    diagnostic.message
                );
            }
            if report.status == DiagnosticStatus::Invalid {
                return Err("input is invalid".into());
            }
        }
        "salvage" => {
            let path = one_path(args, "salvage")?;
            let file = File::open(path)?;
            let mut source = SeekSource::new(file);
            let report = PrefixSalvager::new(Limits::default()).scan(&mut source)?;
            println!(
                "UNVERIFIED prefix salvage: {} complete records, reached_directory={}",
                report.records.len(), report.reached_directory
            );
            for record in report.records {
                println!(
                    "id={} kind={:?} offset={} stored={}",
                    record.object_id, record.kind, record.offset, record.stored_len
                );
            }
            for diagnostic in report.diagnostics {
                println!(
                    "stage={:?} category={:?} offset={:?}: {}",
                    diagnostic.stage,
                    diagnostic.category,
                    diagnostic.offset,
                    diagnostic.message
                );
            }
        }
        "make-demo" => {
            let path = one_path(args, "make-demo")?;
            write_demo(&path)?;
            println!("wrote experimental demo to {}", path.display());
        }
        "help" | "--help" | "-h" => print_help(),
        other => return Err(format!("unknown command {other:?}").into()),
    }
    Ok(())
}

fn one_path(
    mut args: impl Iterator<Item = String>,
    command: &str,
) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let path = args
        .next()
        .ok_or_else(|| format!("{command} requires one file path"))?;
    if args.next().is_some() {
        return Err(format!("{command} accepts exactly one file path").into());
    }
    Ok(path.into())
}

fn write_demo(path: &Path) -> Result<(), Box<dyn Error>> {
    let output = File::create(path)?;
    let mut writer = StreamingWriter::with_default_limits(output)?;
    writer.add_opaque(1, b"hello from UCOF-EXP-0001")?;
    let mut manifest = Manifest::new(vec![1]);
    manifest.optional_capabilities.push(9001);
    writer.add_manifest(2, &manifest)?;
    writer.finish(2)?;
    Ok(())
}

fn print_help() {
    println!(
        "ucof — experimental UCOF-EXP-0001 tool\n\n\
         Usage:\n\
           ucof inspect <file>   metadata only; integrity is not checked\n\
           ucof verify <file>    strict bounded integrity validation\n\
           ucof diagnose <file>  strict status and categorized failure\n\
           ucof salvage <file>   unverified complete-prefix record scan\n\
           ucof make-demo <file> write a deterministic experimental file\n\n\
         Experimental files are disposable and not production compatible."
    );
}
