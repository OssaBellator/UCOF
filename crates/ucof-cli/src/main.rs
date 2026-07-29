use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use ucof_core::{Limits, Manifest, ValidatedFile, Writer};

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
            let bytes = fs::read(path)?;
            let file = ValidatedFile::parse(&bytes, &Limits::default())?;
            print!("{}", file.inspect());
        }
        "verify" => {
            let path = one_path(args, "verify")?;
            let bytes = fs::read(path)?;
            let file = ValidatedFile::parse(&bytes, &Limits::default())?;
            println!(
                "valid UCOF-EXP-0001: {} records, manifest {}",
                file.records.len(),
                file.manifest_id
            );
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

fn one_path(mut args: impl Iterator<Item = String>, command: &str) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let path = args
        .next()
        .ok_or_else(|| format!("{command} requires one file path"))?;
    if args.next().is_some() {
        return Err(format!("{command} accepts exactly one file path").into());
    }
    Ok(path.into())
}

fn write_demo(path: &Path) -> Result<(), Box<dyn Error>> {
    let mut writer = Writer::new();
    writer.add_opaque(1, b"hello from UCOF-EXP-0001")?;
    let mut manifest = Manifest::new(vec![1]);
    manifest.optional_capabilities.push(9001);
    writer.add_manifest(2, &manifest)?;
    let bytes = writer.finish(2)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn print_help() {
    println!(
        "ucof — experimental UCOF-EXP-0001 tool\n\n\
         Usage:\n\
           ucof inspect <file>\n\
           ucof verify <file>\n\
           ucof make-demo <file>\n\n\
         Experimental files are disposable and not production compatible."
    );
}
