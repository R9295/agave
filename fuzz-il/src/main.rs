mod compiler;
mod il;
mod lower;
mod template;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("fuzz-il: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = None;
    let mut output = None;
    let mut emit_c = None;
    let mut compile = true;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            "--emit-c" => {
                let Some(path) = args.next() else {
                    return Err("--emit-c requires a path or `-`".into());
                };
                emit_c = Some(path);
            }
            "--no-compile" | "--emit-c-only" => {
                compile = false;
            }
            "-o" | "--output" => {
                let Some(path) = args.next() else {
                    return Err("--output requires a path".into());
                };
                output = Some(PathBuf::from(path));
            }
            _ if input.is_none() => input = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument `{arg}`").into()),
        }
    }

    let Some(input) = input else {
        print_usage();
        return Err("missing IL input file".into());
    };

    let source = fs::read_to_string(&input)?;
    let c_source = lower::lower_il_to_c(&source)?;

    if let Some(path) = emit_c {
        if path == "-" {
            println!("{c_source}");
        } else {
            fs::write(path, &c_source)?;
        }
    }

    if compile {
        let artifact = compiler::codegen_sbf(&c_source)?;
        let artifact = if let Some(output) = output {
            fs::copy(&artifact, &output)?;
            output
        } else {
            artifact
        };
        println!("{}", display_path(&artifact));
    }

    Ok(())
}

fn print_usage() {
    eprintln!("usage: fuzz-il [--emit-c <path>|-] [--no-compile] [-o <elf.so>] <program.il>");
}

fn display_path(path: &Path) -> String {
    path.to_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}
