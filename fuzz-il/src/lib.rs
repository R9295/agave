//! fuzz-il: an intermediate language for describing single-instruction SVM
//! fuzz fixtures, plus the pipeline that lowers, compiles, and renders them to
//! the protobuf `InstrContext` the conformance harness consumes.
//!
//! Two consumers:
//!   * the `fuzz-il` binary ([`run`]), which compiles one `.il` file, and
//!   * external fuzz harnesses, which use [`instr_context_from_source`] /
//!     [`instr_context_from_lowered`] to turn IL (optionally after a
//!     [`mutator`] pass) into an `InstrContext` in-process.

mod compiler;
pub mod il;
pub mod instr_context;
pub mod lower;
pub mod mutator;
mod template;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub use protosol::protos::InstrContext;

/// Error type for the harness-facing API.
pub type BuildError = Box<dyn std::error::Error>;

/// Parse and lower IL `source` into a [`lower::LoweredProgram`].
///
/// The returned program can be handed to [`mutator`] before being realized
/// into an `InstrContext` with [`instr_context_from_lowered`].
pub fn lower_source(source: &str) -> Result<lower::LoweredProgram, BuildError> {
    Ok(lower::lower_il(source)?)
}

/// Compile a lowered program to an SBF ELF and build its `InstrContext`.
///
/// This shells out to the SBF toolchain (see [`compiler::codegen_sbf`]), so it
/// is not cheap — one clang/lld invocation per call. Callers that mutate the
/// program between lowering and this call get a *consistent* fixture: the ELF
/// and the `InstrContext` account metadata are both derived from `program`.
pub fn instr_context_from_lowered(
    program: &lower::LoweredProgram,
) -> Result<InstrContext, BuildError> {
    let c_source = lower::lowered_to_c(program)?;
    let artifact = compiler::codegen_sbf(&c_source)?;
    let elf_bytes = fs::read(&artifact)?;
    let context = instr_context::lowered_to_instr_context(program, &elf_bytes)?;
    Ok(context)
}

/// Convenience: lower `source` and build its `InstrContext` in one step, with
/// no mutation. Equivalent to `instr_context_from_lowered(&lower_source(..)?)`.
pub fn instr_context_from_source(source: &str) -> Result<InstrContext, BuildError> {
    instr_context_from_lowered(&lower_source(source)?)
}

/// Entry point for the `fuzz-il` CLI binary.
pub fn run() -> Result<(), BuildError> {
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
    let lowered = lower::lower_il(&source)?;
    let c_source = lower::lowered_to_c(&lowered)?;

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
        let elf_bytes = fs::read(&artifact)?;
        instr_context::print_lowered(&lowered, &elf_bytes)?;
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
