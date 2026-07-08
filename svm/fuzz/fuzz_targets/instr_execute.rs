//! Ziggy fuzz target: agave single-instruction execution.
//!
//! Drives the exact code path behind the conformance C ABI entry point
//! `sol_compat_instr_execute_v1` (see `svm/src/conformance/instr/harness.rs`):
//! decode a protobuf `InstrContext` from `org.solana.sealevel.v1`, build the
//! transaction / invoke context, execute the single instruction against the
//! agave SVM, and compute its `InstrEffects`. We call `execute_instr_proto`
//! directly — the Rust function that the FFI wraps — so the fuzzer drives an
//! in-process harness with no output-buffer marshalling.
//!
//! Engine: srlabs ziggy (AFL++ / honggfuzz), NOT cargo-fuzz / afl.rs.
//!
//!   cargo ziggy build                        # build the instrumented target
//!   cargo ziggy fuzz                          # fuzz
//!   cargo ziggy run ./output/.../crashes/<id> # replay one crashing input
//!
//! Seed the corpus first (see ../README.md):
//!   cargo run --release --example gen_corpus -- corpus/instr_execute

use {
    prost::Message,
    protosol::protos::{FeatureSet, InstrContext},
    solana_sdk_ids::{
        bpf_loader, bpf_loader_deprecated, bpf_loader_upgradeable, compute_budget, system_program,
        sysvar, vote, zk_elgamal_proof_program,
    },
    solana_svm::conformance::instr::harness::execute_instr_proto,
    std::{
        collections::HashSet,
        sync::LazyLock,
    },
};

/// Every gateable feature id, packed the way the conformance converter expects:
/// the first 8 bytes of the feature pubkey, little-endian (see
/// `svm/src/conformance/feature_set.rs::feature_u64`). We force this onto every
/// input so the fuzzer always runs against the *latest* runtime — otherwise an
/// input with `features` unset/empty would execute with every feature OFF (the
/// genesis config), which is not what a current validator does.
static ALL_FEATURES: LazyLock<Vec<u64>> = LazyLock::new(|| {
    agave_feature_set::FEATURE_NAMES
        .keys()
        .map(|pubkey| u64::from_le_bytes(pubkey.to_bytes()[..8].try_into().unwrap()))
        .collect()
});

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        // (1) Decode the protobuf InstrContext. Malformed protobuf is not a
        //     finding: the real `sol_compat_instr_execute_v1` also bails out
        //     here (it returns 0).
        let Ok(mut ctx) = InstrContext::decode(data) else {
            eprintln!("error decoding protobuf");
            return;
        };

        // (3) Hardcode all features ON, overriding whatever the input carried,
        //     so every execution targets the latest runtime. This makes the
        //     `features` field non-fuzzable by design — remove this to let the
        //     fuzzer explore feature-gating instead.
        ctx.features = Some(FeatureSet {
            features: ALL_FEATURES.clone(),
        });

        // (4) Execute. No catch_unwind — we *want* ziggy to capture panics,
        //     aborts, and UB past this point.
        let effects = execute_instr_proto(ctx);
        eprintln!("{:?}", effects);
    });
}
