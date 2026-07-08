//! Generate a minimal valid seed corpus for the `instr_execute` ziggy target.
//!
//! Writes protobuf-encoded `InstrContext` fixtures that pass the harness
//! preconditions, giving `cargo ziggy fuzz` structurally valid starting points
//! to mutate (coverage-guided fuzzing from random bytes alone almost never
//! produces a decodable, executable InstrContext).
//!
//!   cargo run --release --example gen_corpus -- corpus/instr_execute
//!
//! As a side effect this self-tests the whole pipeline: every generated fixture
//! is executed through `execute_instr_proto` and its effect is printed, so a
//! broken harness shows up here long before you start a fuzzing campaign.

use {
    prost::Message,
    protosol::protos::{AcctState, InstrAcct, InstrContext},
    solana_sdk_ids::{native_loader, system_program, sysvar},
    solana_svm::conformance::instr::harness::execute_instr_proto,
    std::{fs, path::Path},
};

fn acct(address: [u8; 32], owner: [u8; 32], lamports: u64, data: Vec<u8>) -> AcctState {
    AcctState {
        address: address.to_vec(),
        owner: owner.to_vec(),
        lamports,
        data,
        ..Default::default()
    }
}

fn clock_account() -> AcctState {
    let data = bincode::serialize(&solana_clock::Clock::default()).unwrap();
    acct(sysvar::clock::id().to_bytes(), sysvar::id().to_bytes(), 1, data)
}

fn rent_account() -> AcctState {
    let data = bincode::serialize(&solana_rent::Rent::default()).unwrap();
    acct(sysvar::rent::id().to_bytes(), sysvar::id().to_bytes(), 1, data)
}

/// A System-program transfer of `amount` lamports from account 0 to account 1.
fn system_transfer(amount: u64) -> InstrContext {
    let from = [1u8; 32];
    let to = [2u8; 32];
    let system = system_program::id().to_bytes();

    // SystemInstruction::Transfer { lamports }: bincode = u32 variant tag (2)
    // followed by the u64 amount, both little-endian.
    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend_from_slice(&amount.to_le_bytes());

    InstrContext {
        program_id: system.to_vec(),
        // Order matters: instr_accounts index into this list.
        accounts: vec![
            acct(from, system, amount + 5_000, vec![]), // 0: funding source
            acct(to, system, 1_000, vec![]),            // 1: recipient
            // The System program account (owned by the native loader). It is
            // already a builtin in the cache, but real fixtures carry it too.
            acct(system, native_loader::id().to_bytes(), 1, b"system_program".to_vec()),
            clock_account(),
            rent_account(),
        ],
        instr_accounts: vec![
            InstrAcct { index: 0, is_signer: true, is_writable: true },
            InstrAcct { index: 1, is_signer: false, is_writable: true },
        ],
        data,
        cu_avail: 200_000,
        ..Default::default()
    }
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "corpus/instr_execute".to_string());
    let dir = Path::new(&out);
    fs::create_dir_all(dir).expect("create corpus dir");

    let seeds: &[(&str, InstrContext)] = &[
        ("system_transfer_1000", system_transfer(1_000)),
        ("system_transfer_1", system_transfer(1)),
    ];

    for (name, ctx) in seeds {
        // Self-test: run the same path the fuzz target hits, and report the
        // outcome so a regression in the harness is obvious immediately.
        let effects = execute_instr_proto(ctx.clone());
        println!(
            "seed {name:>22}: result={} cu_avail={} modified_accounts={}",
            effects.result,
            effects.cu_avail,
            effects.modified_accounts.len(),
        );

        let path = dir.join(format!("{name}.bin"));
        fs::write(&path, ctx.encode_to_vec()).expect("write seed");
        println!("  -> wrote {} ({} bytes)", path.display(), ctx.encoded_len());
    }
}
