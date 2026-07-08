//! Ziggy fuzz target: mutate an IL fixture, then execute it.
//!
//! Unlike `instr_execute` (which decodes a raw protobuf `InstrContext` straight
//! from the fuzzer bytes), this target treats the fuzzer input as a *recipe*:
//!
//!   1. pick one `.il` testcase from `$CORPUS_PATH` (a directory of fuzz-il
//!      source files),
//!   2. lower it and apply one structural mutation — flipping `is_signer` or
//!      `is_writable` on an account meta of some call (system-program CPI or
//!      direct-manipulation call alike, via `fuzz_il::mutator`),
//!   3. compile + realize the mutated program into an `InstrContext`, and
//!   4. run it through the same `execute_instr_proto` path as `instr_execute`.
//!
//! This keeps every execution anchored to a hand-written, semantically valid
//! fixture while the fuzzer explores the signer/writable privilege space around
//! it — the class of divergence that raw-protobuf fuzzing rarely reaches
//! because it almost never produces a well-formed instruction.
//!
//! Engine: srlabs ziggy (AFL++ / honggfuzz), NOT cargo-fuzz / afl.rs.
//!
//!   CORPUS_PATH=../../fuzz-il/testcases cargo ziggy build
//!   CORPUS_PATH=../../fuzz-il/testcases cargo ziggy fuzz
//!
//! Note: `CORPUS_PATH` points at IL *source* files (the mutation seed pool),
//! which is distinct from ziggy's own byte-level corpus under `./output`.

use {
    fuzz_il::{
        instr_context_from_lowered, lower_source,
        mutator::{flip_invocation_is_signer, flip_invocation_is_writable},
        InstrContext,
    },
    protosol::protos::FeatureSet,
    std::{
        fs,
        path::PathBuf,
        sync::LazyLock,
    },
};

/// Every gateable feature id, packed the way the conformance converter expects
/// (first 8 bytes of the feature pubkey, little-endian). Mirrors
/// `instr_execute.rs`: we force all features ON so executions target the latest
/// runtime instead of the genesis (all-off) config.
static ALL_FEATURES: LazyLock<Vec<u64>> = LazyLock::new(|| {
    agave_feature_set::FEATURE_NAMES
        .keys()
        .map(|pubkey| u64::from_le_bytes(pubkey.to_bytes()[..8].try_into().unwrap()))
        .collect()
});

/// The IL source files under `$CORPUS_PATH`, sorted for a stable index→file
/// mapping across runs. Read once. Panics if `CORPUS_PATH` is unset or empty —
/// a misconfigured harness is a setup error, not a fuzz finding.
static TESTCASES: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    let dir = std::env::var_os("CORPUS_PATH")
        .expect("CORPUS_PATH must point at a directory of .il testcases");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading CORPUS_PATH {dir:?}: {e}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "il"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no .il testcases found under CORPUS_PATH {dir:?}"
    );
    files
});

fn main() {
    // Warm the corpus list up front so a config error surfaces immediately
    // rather than inside the first fuzz iteration.
    LazyLock::force(&TESTCASES);

    ziggy::fuzz!(|data: &[u8]| {
        // The recipe needs a few control bytes; anything shorter can't select a
        // testcase + mutation, so treat it as an uninteresting no-op.
        let [file_sel, mut_sel, seed_bytes @ ..] = data else {
            return;
        };

        // (1) Pick the seed testcase.
        let path = &TESTCASES[*file_sel as usize % TESTCASES.len()];
        let Ok(source) = fs::read_to_string(path) else {
            return;
        };

        // (2) Lower it. A corpus file that no longer parses/lowers is a corpus
        //     bug, not a finding — skip rather than crash the fuzzer.
        let Ok(mut program) = lower_source(&source) else {
            return;
        };

        // (3) Apply one mutation, selected by the fuzzer. The low bit picks the
        //     flag (signer vs writable); the remaining bytes pick which account
        //     meta (mod the number of metas across all invocations). If the
        //     program has no account metas, the flip is a no-op and we execute
        //     the fixture unchanged.
        let seed = seed_bytes
            .iter()
            .fold(0usize, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as usize));
        if mut_sel & 1 == 0 {
            flip_invocation_is_signer(&mut program, seed);
        } else {
            flip_invocation_is_writable(&mut program, seed);
        }

        // (4) Realize the mutated program into an InstrContext. Compiling the
        //     harness ELF can legitimately fail for some inputs; that is a
        //     lowering/toolchain outcome, not an execution finding.
        let Ok(mut ctx): Result<InstrContext, _> = instr_context_from_lowered(&program) else {
            return;
        };

        // Force all features ON, matching instr_execute.
        ctx.features = Some(FeatureSet {
            features: ALL_FEATURES.clone(),
        });

        // (5) Execute. No catch_unwind — we want ziggy to capture panics past
        //     this point.
        let effects = solana_svm::conformance::instr::harness::execute_instr_proto(ctx);
        eprintln!("{:?}", effects);
    });
}
