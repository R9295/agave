use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::SystemTime,
};

/// Compile a C source string into a Solana BPF (SBF) shared object.
///
/// Pipeline:
///   clang-20 -cc1 -triple sbf -target-cpu v3 -emit-obj …
///   llvm-objcopy --remove-section .eh_frame
///   ld.lld   -shared -z notext -z max-page-size=8 --no-rosegment -T <script>
///   post-link: rewrite `.rel.dyn` `R_BPF_64_32` relocations into
///   static-syscall call sites (stamps `hash_symbol_name(name)` into the
///   imm field of each referencing `call` in `.text`). SBPFv3 has no
///   dynamic linker at run time — calls are resolved by hash.
///
/// Intermediate `.c` / `.o` / `.ld` live in `/dev/shm` on Linux and
/// `$TMPDIR` elsewhere; per-pid + nanos stem keeps concurrent callers
/// from colliding. Returns the path to the produced `.so`.
pub fn codegen_sbf(c_source: &str) -> std::io::Result<PathBuf> {
    let dir = temp_artifact_dir();
    let stem = format!("fuzz-{}-{}", std::process::id(), now_nanos());
    let src = dir.join(format!("{stem}.c"));
    let obj = dir.join(format!("{stem}.o"));
    let ld_script = dir.join(format!("{stem}.ld"));
    let so = dir.join(format!("{stem}.so"));

    std::fs::write(&src, c_source)?;
    std::fs::write(&ld_script, V3_LINKER_SCRIPT)?;

    run(
        &llvm_bin().join("clang-20"),
        &[
            "-cc1".as_ref(),
            "-triple".as_ref(),
            "sbf".as_ref(),
            "-target-cpu".as_ref(),
            "v3".as_ref(),
            "-emit-obj".as_ref(),
            "-O0".as_ref(),
            "-fno-builtin".as_ref(),
            "-disable-free".as_ref(),
            "-w".as_ref(),
            "-x".as_ref(),
            "c".as_ref(),
            src.as_os_str(),
            "-o".as_ref(),
            obj.as_os_str(),
        ],
    )?;

    // Strip `.eh_frame` from the object — clang emits it even at `-O0` and
    // ld.lld would otherwise either complain or place it in a way the v3
    // parser rejects.
    run(
        &llvm_bin().join("llvm-objcopy"),
        &[
            "--remove-section".as_ref(),
            ".eh_frame".as_ref(),
            obj.as_os_str(),
        ],
    )?;

    run(
        &llvm_bin().join("ld.lld"),
        &[
            "-shared".as_ref(),
            "-z".as_ref(),
            "notext".as_ref(),
            // INSN_SIZE = 8, so the strict v3 parser requires every segment
            // offset / size to be 8-aligned. Default page size 4K would force
            // ld.lld to pad to 4K and emit non-conforming offsets.
            "-z".as_ref(),
            "max-page-size=8".as_ref(),
            // Keep `.text` and `.rodata` in their own segments rather than
            // merging into a single R+X "rosegment" — the v3 parser wants
            // bytecode marked `PF_X` only, no `PF_R`.
            "--no-rosegment".as_ref(),
            "-T".as_ref(),
            ld_script.as_os_str(),
            "--entry".as_ref(),
            "entrypoint".as_ref(),
            obj.as_os_str(),
            "-o".as_ref(),
            so.as_os_str(),
        ],
    )?;

    // Post-link: resolve dynamic-syscall relocations to static-syscall hashes
    // so the runtime can bind `sol_memcpy_` / `sol_log_` / etc. without a
    // dynamic linker. Mutates the .so in place.
    {
        let mut bytes = std::fs::read(&so)?;
        patch_static_syscalls(&mut bytes)?;
        std::fs::write(&so, &bytes)?;
    }

    // .c, .o and .ld are intermediate; remove them but leave the .so for the caller.
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&obj);
    let _ = std::fs::remove_file(&ld_script);

    Ok(so)
}

/// Returns the directory containing Solana's prebuilt llvm tools
/// (clang-20, ld.lld, ...).
///
/// Resolution order:
///   1. `SOLANA_LLVM_BIN` env var, if set.
///   2. The newest `v*` install under `$HOME/.cache/solana/` whose
///      `platform-tools/llvm/bin/clang-20` exists.
fn llvm_bin() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(resolve_llvm_bin).as_path()
}

fn resolve_llvm_bin() -> PathBuf {
    if let Ok(p) = std::env::var("SOLANA_LLVM_BIN") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").expect("HOME not set");
    let cache = Path::new(&home).join(".cache/solana");
    let mut versions: Vec<(Vec<u32>, String)> = std::fs::read_dir(&cache)
        .unwrap_or_else(|e| panic!("reading {}: {e}", cache.display()))
        .filter_map(|r| r.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with('v'))
        .map(|n| (parse_version(&n), n))
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
    for (_, v) in versions {
        let bin = cache.join(&v).join("platform-tools/llvm/bin");
        if bin.join("clang-20").is_file() {
            return bin;
        }
    }
    panic!(
        "no Solana platform-tools llvm install found under {}; \
         install via `cargo-build-sbf` or set SOLANA_LLVM_BIN",
        cache.display()
    );
}

fn parse_version(s: &str) -> Vec<u32> {
    s.trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// SBPFv3 linker script. The strict v3 ELF parser only validates `PT_LOAD`
/// segments — so the dyn tables we still need for the static-syscall patch
/// (`.dynsym` / `.dynstr` / `.rel.dyn`) live in a `PT_NOTE` segment, which
/// the parser ignores. Layout:
///   * rodata at vaddr 0           (PF_R)
///   * text   at vaddr 0x100000000 (PF_X)  — `MM_BYTECODE_START`
///   * dyn    at vaddr 0x200000000 (PT_NOTE, not loaded by VM)
///
/// Lifted from `~/projects/anza/syscall-fuzzer/crates/compile/src/lib.rs`.
const V3_LINKER_SCRIPT: &str = r#"PHDRS {
  rodata PT_LOAD FLAGS(4);
  text PT_LOAD FLAGS(1);
  dyn PT_NOTE FLAGS(0);
}

ENTRY(entrypoint)

SECTIONS {
  . = 0;
  .rodata : ALIGN(8) SUBALIGN(8) {
    QUAD(0);
    *(.rodata) *(.rodata.*)
    . = ALIGN(8);
  } :rodata

  . = 0x100000000;
  .text : ALIGN(8) SUBALIGN(8) { *(.text) *(.text.*) . = ALIGN(8); } :text

  . = 0x200000000;
  .dynsym : { *(.dynsym) } :dyn
  .dynstr : { *(.dynstr) } :dyn
  .rel.dyn : { *(.rel.dyn) } :dyn

  /DISCARD/ : {
    *(.dynamic) *(.eh_frame)
    *(.gnu.hash) *(.hash) *(.rela.*)
    *(.comment) *(.symtab) *(.strtab) *(.relro_padding)
    *(.note.*)
  }
}
"#;

const R_BPF_64_32: u32 = 10;

/// Rewrite every `R_BPF_64_32` relocation in `.rel.dyn` into a static-syscall
/// call site: look up the referenced symbol name in `.dynsym` / `.dynstr`,
/// hash it via `solana_sbpf::ebpf::hash_symbol_name`, and stamp the hash
/// into the call instruction at the relocation's vaddr. SBPFv3 resolves
/// syscalls by this hash; without this pass, the loader has no way to bind
/// `sol_memcpy_` / `sol_log_` / etc. to runtime implementations.
///
/// Ported verbatim from `~/projects/anza/syscall-fuzzer/crates/compile/src/lib.rs`.
fn patch_static_syscalls(elf: &mut [u8]) -> std::io::Result<()> {
    let text = locate_text_segment(elf)
        .ok_or_else(|| std::io::Error::other("no PF_X PT_LOAD segment found in ELF"))?;
    let dynsym = section_by_name(elf, ".dynsym");
    let dynstr = section_by_name(elf, ".dynstr");
    let rel_dyn = section_by_name(elf, ".rel.dyn");

    let (
        Some((dynsym_off, dynsym_size)),
        Some((dynstr_off, dynstr_size)),
        Some((rel_off, rel_size)),
    ) = (dynsym, dynstr, rel_dyn)
    else {
        return Ok(());
    };

    let dynstr_end = dynstr_off + dynstr_size;
    if dynstr_end > elf.len()
        || dynsym_off + dynsym_size > elf.len()
        || rel_off + rel_size > elf.len()
    {
        return Err(std::io::Error::other("dynamic section out of bounds"));
    }

    let entry_count = rel_size / 16;
    for i in 0..entry_count {
        let base = rel_off + i * 16;
        let r_offset = u64::from_le_bytes(elf[base..base + 8].try_into().unwrap());
        let r_info = u64::from_le_bytes(elf[base + 8..base + 16].try_into().unwrap());
        let r_type = (r_info & 0xffff_ffff) as u32;
        let r_sym = (r_info >> 32) as u32;
        if r_type != R_BPF_64_32 {
            continue;
        }

        let sym_base = dynsym_off + (r_sym as usize) * 24;
        if sym_base + 24 > dynsym_off + dynsym_size {
            return Err(std::io::Error::other("symbol index out of range"));
        }
        let st_name = u32::from_le_bytes(elf[sym_base..sym_base + 4].try_into().unwrap());

        let name_start = dynstr_off + (st_name as usize);
        if name_start >= dynstr_end {
            return Err(std::io::Error::other("symbol name out of range"));
        }
        let mut name_end = name_start;
        while name_end < dynstr_end && elf[name_end] != 0 {
            name_end += 1;
        }
        let name = &elf[name_start..name_end];
        let hash = solana_sbpf::ebpf::hash_symbol_name(name);

        let file_off = vaddr_to_file_offset(&text, r_offset).ok_or_else(|| {
            std::io::Error::other(format!("reloc r_offset 0x{r_offset:x} not in text segment"))
        })?;
        if file_off + 8 > elf.len() {
            return Err(std::io::Error::other("reloc target past EOF"));
        }
        // Clear src-register nibble of the call instruction's reg byte; then
        // stamp the 32-bit symbol hash into the imm field.
        elf[file_off + 1] &= 0x0F;
        elf[file_off + 4..file_off + 8].copy_from_slice(&hash.to_le_bytes());
    }

    Ok(())
}

struct TextSegment {
    offset: usize,
    filesz: usize,
    vaddr: u64,
}

fn vaddr_to_file_offset(text: &TextSegment, vaddr: u64) -> Option<usize> {
    if vaddr < text.vaddr {
        return None;
    }
    let delta = vaddr - text.vaddr;
    if delta as usize >= text.filesz {
        return None;
    }
    Some(text.offset + delta as usize)
}

fn locate_text_segment(elf: &[u8]) -> Option<TextSegment> {
    if elf.len() < 64 || &elf[0..4] != b"\x7fELF" {
        return None;
    }
    let e_phoff = u64::from_le_bytes(elf[0x20..0x28].try_into().ok()?) as usize;
    let e_phentsize = u16::from_le_bytes(elf[0x36..0x38].try_into().ok()?) as usize;
    let e_phnum = u16::from_le_bytes(elf[0x38..0x3a].try_into().ok()?) as usize;
    for i in 0..e_phnum {
        let base = e_phoff + i * e_phentsize;
        if base + 56 > elf.len() {
            return None;
        }
        let p_type = u32::from_le_bytes(elf[base..base + 4].try_into().ok()?);
        let p_flags = u32::from_le_bytes(elf[base + 4..base + 8].try_into().ok()?);
        // PT_LOAD = 1, PF_X = 1 (we want bytecode segment: PF_X only)
        if p_type == 1 && p_flags == 1 {
            let p_offset = u64::from_le_bytes(elf[base + 8..base + 16].try_into().ok()?) as usize;
            let p_vaddr = u64::from_le_bytes(elf[base + 16..base + 24].try_into().ok()?);
            let p_filesz = u64::from_le_bytes(elf[base + 32..base + 40].try_into().ok()?) as usize;
            return Some(TextSegment {
                offset: p_offset,
                filesz: p_filesz,
                vaddr: p_vaddr,
            });
        }
    }
    None
}

fn section_by_name(elf: &[u8], name: &str) -> Option<(usize, usize)> {
    if elf.len() < 64 {
        return None;
    }
    let e_shoff = u64::from_le_bytes(elf[0x28..0x30].try_into().ok()?) as usize;
    let e_shentsize = u16::from_le_bytes(elf[0x3a..0x3c].try_into().ok()?) as usize;
    let e_shnum = u16::from_le_bytes(elf[0x3c..0x3e].try_into().ok()?) as usize;
    let e_shstrndx = u16::from_le_bytes(elf[0x3e..0x40].try_into().ok()?) as usize;

    let shstr_hdr = e_shoff + e_shstrndx * e_shentsize;
    if shstr_hdr + e_shentsize > elf.len() {
        return None;
    }
    let shstr_off =
        u64::from_le_bytes(elf[shstr_hdr + 24..shstr_hdr + 32].try_into().ok()?) as usize;
    let shstr_size =
        u64::from_le_bytes(elf[shstr_hdr + 32..shstr_hdr + 40].try_into().ok()?) as usize;
    if shstr_off + shstr_size > elf.len() {
        return None;
    }

    for i in 0..e_shnum {
        let base = e_shoff + i * e_shentsize;
        if base + e_shentsize > elf.len() {
            return None;
        }
        let sh_name_off = u32::from_le_bytes(elf[base..base + 4].try_into().ok()?) as usize;
        let abs_name_off = shstr_off + sh_name_off;
        if abs_name_off >= shstr_off + shstr_size {
            continue;
        }
        let mut end = abs_name_off;
        while end < shstr_off + shstr_size && elf[end] != 0 {
            end += 1;
        }
        if &elf[abs_name_off..end] == name.as_bytes() {
            let sh_offset = u64::from_le_bytes(elf[base + 24..base + 32].try_into().ok()?) as usize;
            let sh_size = u64::from_le_bytes(elf[base + 32..base + 40].try_into().ok()?) as usize;
            return Some((sh_offset, sh_size));
        }
    }
    None
}

fn temp_artifact_dir() -> PathBuf {
    if cfg!(target_os = "linux") {
        let shm = PathBuf::from("/dev/shm");
        if shm.is_dir() {
            return shm;
        }
    }
    std::env::temp_dir()
}

fn run(prog: &Path, args: &[&std::ffi::OsStr]) -> std::io::Result<()> {
    // Capture stdout/stderr so warnings stay hidden on success; on failure
    // they're folded into the error message so we still see what broke.
    let out = Command::new(prog).args(args).output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(format!(
            "{} exited with {}\n--- stderr ---\n{}\n--- stdout ---\n{}",
            prog.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout),
        )));
    }
    Ok(())
}

pub fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
