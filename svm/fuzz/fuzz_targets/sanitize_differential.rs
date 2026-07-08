//! Ziggy fuzz target: differential sanitization (VIEW vs LEGACY).
//!
//! Agave sanitizes the *same* transaction wire bytes with two independent
//! implementations on two different consensus-relevant paths:
//!
//!   * VIEW   — zero-copy `agave_transaction_view::SanitizedTransactionView`
//!              (`transaction-view/src/sanitize.rs`). Used by the leader's
//!              banking stage, sigverify, forwarding.
//!   * LEGACY — SDK `Sanitize` / `v1::Message::validate()`
//!              (`solana-message`, `solana-transaction`). Used by BLOCK REPLAY
//!              (consensus), RPC, banks-server.
//!
//! If those two ever disagree on whether a byte string is a valid transaction
//! — or agree it's valid but decode it into different fields — a leader can
//! pack a block that replica nodes reject (or vice-versa): a fork / dead block.
//! This harness asserts ACCEPT/REJECT parity and, on mutual-accept, structural
//! parity. Any divergence panics, which ziggy records as a crash.
//!
//! Scope: this is PHASE 1 — static sanitize only (no bank, no ALT resolution).
//! See the `PHASE 2 / 3` notes at the bottom for the resolved-address and
//! full-`RuntimeTransaction` extensions (those need a mock account loader).
//!
//! Engine: srlabs ziggy (AFL++ / honggfuzz), NOT cargo-fuzz / afl.rs.
//!
//!   cargo ziggy build
//!   cargo ziggy fuzz -t sanitize_differential
//!   cargo ziggy run ./output/.../crashes/<id>     # replay a divergence
//!
//! Seed corpus with real serialized transactions (see README / gen_corpus).

use {
    agave_feature_set::FeatureSet,
    agave_transaction_view::{
        transaction_version::TransactionVersion as ViewVersion,
        transaction_view::SanitizedTransactionView,
    },
    arbitrary::{Arbitrary, Unstructured},
    solana_hash::Hash,
    solana_message::{
        compiled_instruction::CompiledInstruction, v0::MessageAddressTableLookup, MessageHeader,
        VersionedMessage,
    },
    solana_packet::PACKET_DATA_SIZE,
    solana_pubkey::Pubkey,
    solana_runtime_transaction::{
        runtime_transaction::RuntimeTransaction, transaction_meta::TransactionMeta,
    },
    solana_signature::Signature,
    solana_transaction::{
        sanitized::MessageHash,
        versioned::{
            sanitized::SanitizedVersionedTransaction, TransactionVersion as SdkVersion,
            VersionedTransaction,
        },
    },
    solana_transaction_context::{MAX_ACCOUNTS_PER_INSTRUCTION, MAX_INSTRUCTION_TRACE_LENGTH},
    std::sync::LazyLock,
};

/// Normalized, implementation-agnostic projection of a sanitized transaction.
/// Both sanitizers must produce an identical value when they both accept.
// Native solana types (Pubkey/Hash/Signature derive Eq+Clone+Copy) — and the
// view and legacy sides resolve to the SAME crate versions, so we compare them
// directly with no byte conversions.
#[derive(Debug, PartialEq, Eq)]
struct Normalized {
    version: u8, // 0 = legacy, 1 = v0, 2 = v1  (see `vv`/`sv` below)
    num_required_signatures: u8,
    num_readonly_signed: u8,
    num_readonly_unsigned: u8,
    static_keys: Vec<Pubkey>,
    recent_blockhash: Hash,
    signatures: Vec<Signature>,
    // (program_id_index, account_indexes, data)
    instructions: Vec<(u8, Vec<u8>, Vec<u8>)>,
    // (table_key, writable_indexes, readonly_indexes)
    atls: Vec<(Pubkey, Vec<u8>, Vec<u8>)>,
}

fn vv(v: ViewVersion) -> u8 {
    match v {
        ViewVersion::Legacy => 0,
        ViewVersion::V0 => 1,
        ViewVersion::V1 => 2,
    }
}
fn sv(v: SdkVersion) -> u8 {
    match v {
        SdkVersion::Legacy(_) => 0,
        SdkVersion::Number(0) => 1,
        SdkVersion::Number(1) => 2,
        SdkVersion::Number(_) => 0xff, // unreachable for sanitized txs
    }
}

// ---------------------------------------------------------------------------
// VIEW side
// ---------------------------------------------------------------------------

fn view_sanitize(wire: &[u8], enable_ix_acct_limit: bool) -> Option<SanitizedTransactionView<&[u8]>> {
    SanitizedTransactionView::try_new_sanitized(wire, enable_ix_acct_limit).ok()
}

fn normalize_view(v: &SanitizedTransactionView<&[u8]>) -> Normalized {
    // NB: accessor names below are the ones used in transaction-view/src/sanitize.rs
    //     and the *_frame modules; double-check against the crate's public API.
    Normalized {
        version: vv(v.version()),
        num_required_signatures: v.num_required_signatures(),
        num_readonly_signed: v.num_readonly_signed_static_accounts(),
        num_readonly_unsigned: v.num_readonly_unsigned_static_accounts(),
        static_keys: v.static_account_keys().to_vec(),
        recent_blockhash: *v.recent_blockhash(),
        signatures: v.signatures().to_vec(),
        instructions: v
            .instructions_iter()
            .map(|ix| (ix.program_id_index, ix.accounts.to_vec(), ix.data.to_vec()))
            .collect(),
        atls: v
            .address_table_lookup_iter()
            .map(|a| {
                (
                    *a.account_key,
                    a.writable_indexes.to_vec(),
                    a.readonly_indexes.to_vec(),
                )
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// LEGACY side — mirrors the consensus replay pipeline:
//   verify_transaction_with_serialized_message (bank.rs): size precheck, then
//   RuntimeTransaction::try_create -> SanitizedVersionedTransaction + Sanitize,
//   then the SIMD-406 per-instruction account limit (flag-gated).
// We already hold the decoded `tx`, so we sanitize it directly.
// ---------------------------------------------------------------------------

fn legacy_sanitize(tx: &VersionedTransaction, wire_len: usize, enable_ix_acct_limit: bool) -> bool {
    // (a) size precheck — the bank applies this BEFORE try_create. Without it
    //     we'd report spurious divergences (e.g. the legacy SDK lacks the
    //     <=12-signature check, but it is masked by size for legacy/v0).
    let max = match tx.version() {
        SdkVersion::Number(1) => solana_message::v1::MAX_TRANSACTION_SIZE,
        _ => PACKET_DATA_SIZE,
    };
    if wire_len > max {
        return false;
    }
    // (a2) SIMD-160 (bank.rs:5430): the replay path rejects >64 top-level
    // instructions for ALL versions, OUTSIDE the SDK sanitize(). The view
    // enforces this inside sanitize (sanitize.rs:139), so we must mirror it or
    // we falsely diverge on a tx the real consensus path also rejects.
    if tx.message.instructions().len() > MAX_INSTRUCTION_TRACE_LENGTH {
        return false;
    }
    // (b) signature-level sanitize (sanitize_signatures).
    if SanitizedVersionedTransaction::try_new(tx.clone()).is_err() {
        return false;
    }
    // (c) structural sanitize: VersionedMessage -> per-version Sanitize
    //     (for v1 this is `v1::Message::validate()`).
    if tx.message.sanitize().is_err() {
        return false;
    }
    // (d) SIMD-406, flag-gated (mirrors RuntimeTransaction::try_create).
    if enable_ix_acct_limit
        && tx
            .message
            .instructions()
            .iter()
            .any(|ix| ix.accounts.len() > MAX_ACCOUNTS_PER_INSTRUCTION)
    {
        return false;
    }
    true
}

fn normalize_legacy(tx: &VersionedTransaction) -> Normalized {
    let h: &MessageHeader = tx.message.header();
    Normalized {
        version: sv(tx.version()),
        num_required_signatures: h.num_required_signatures,
        num_readonly_signed: h.num_readonly_signed_accounts,
        num_readonly_unsigned: h.num_readonly_unsigned_accounts,
        static_keys: tx.message.static_account_keys().to_vec(),
        recent_blockhash: *tx.message.recent_blockhash(),
        signatures: tx.signatures.clone(),
        instructions: tx
            .message
            .instructions()
            .iter()
            .map(|ix: &CompiledInstruction| {
                (ix.program_id_index, ix.accounts.clone(), ix.data.clone())
            })
            .collect(),
        atls: tx
            .message
            .address_table_lookups()
            .map(|atls| {
                atls.iter()
                    .map(|a: &MessageAddressTableLookup| {
                        (
                            a.account_key,
                            a.writable_indexes.clone(),
                            a.readonly_indexes.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// Input generation. Raw fuzz bytes almost never parse, so we drive a structured
// generator that builds a VersionedTransaction biased toward the validation
// boundaries (counts at limits, duplicate keys, edge program/account indices),
// serialize it to wire format, and feed the SAME bytes to both sanitizers.
//
// Because we built `tx` ourselves we use it directly as the LEGACY input and
// only the VIEW side re-parses the wire — so this primarily fuzzes the two
// *sanitize* implementations. To additionally fuzz the two *parsers* against
// each other, add a mode that `wincode::deserialize`s `wire` on the legacy side
// too and compares decode results (parser-level differential).
// ---------------------------------------------------------------------------

#[derive(Arbitrary, Debug)]
struct Recipe {
    version: u8, // % 3 -> legacy / v0 / v1
    num_signatures: u8,
    num_readonly_signed: u8,
    num_readonly_unsigned: u8,
    num_keys: u8,
    dup_key: bool, // force a duplicate static key (exercises the deferred dup check)
    instrs: Vec<(u8, Vec<u8>, Vec<u8>)>, // (program_id_index, accounts, data)
    atls: Vec<([u8; 32], Vec<u8>, Vec<u8>)>,
    blockhash: [u8; 32],
    // PARSER-LEVEL fuzzing: for v1 txs, overwrite the 4-byte
    // TransactionConfigMask in the wire with an arbitrary value. This exercises
    // the v1 config-mask decode, where the VIEW (`sanitize_mask`) rejects
    // unknown / half-set-priority masks at parse while replay's wincode decoder
    // silently normalizes them. A malicious leader writes block bytes directly,
    // so replay consumes attacker-chosen masks.
    mutate_v1_mask: bool,
    raw_v1_mask: u32,
}

fn build_tx(r: &Recipe) -> Option<VersionedTransaction> {
    // Bias counts toward small/boundary values so we stay near validity.
    let n_keys = (r.num_keys % 70).max(1);
    let mut keys: Vec<Pubkey> = (0..n_keys).map(|_| Pubkey::new_unique()).collect();
    if r.dup_key && keys.len() >= 2 {
        keys[1] = keys[0]; // duplicate -> view defers, legacy v1 rejects inline
    }
    let header = MessageHeader {
        num_required_signatures: (r.num_signatures % 14).max(1),
        num_readonly_signed_accounts: r.num_readonly_signed % 14,
        num_readonly_unsigned_accounts: r.num_readonly_unsigned % 14,
    };
    let instructions: Vec<CompiledInstruction> = r
        .instrs
        .iter()
        .take(70)
        .map(|(pid, accts, data)| CompiledInstruction {
            program_id_index: *pid,
            // Cap at 255: the v1 wire encodes per-instruction account count as a
            // u8 (`accounts.len() as u8`, solana-message v1/message.rs:579, no
            // bounds check), so >255 accounts would TRUNCATE on serialize and
            // produce a non-round-trippable wire (header count != body length) —
            // a generator artifact, not an agave divergence. Legacy/v0 use a
            // compact-u16 count and would tolerate more, but 255 keeps every
            // generated tx faithfully serializable across all versions.
            accounts: accts.iter().take(255).copied().collect(),
            data: data.iter().take(64).copied().collect(),
        })
        .collect();
    let blockhash = Hash::new_from_array(r.blockhash);
    let sigs = vec![Signature::default(); header.num_required_signatures as usize];

    let message = match r.version % 3 {
        0 => VersionedMessage::Legacy(solana_message::Message {
            header,
            account_keys: keys,
            recent_blockhash: blockhash,
            instructions,
        }),
        1 => VersionedMessage::V0(solana_message::v0::Message {
            header,
            account_keys: keys,
            recent_blockhash: blockhash,
            instructions,
            address_table_lookups: r
                .atls
                .iter()
                .take(40)
                .map(|(k, w, ro)| MessageAddressTableLookup {
                    account_key: Pubkey::new_from_array(*k),
                    writable_indexes: w.iter().take(70).copied().collect(),
                    readonly_indexes: ro.iter().take(70).copied().collect(),
                })
                .collect(),
        }),
        _ => {
            // v1: no ATLs; carries a TransactionConfig. Keep config empty here;
            // extend the Recipe to fuzz the config mask / heap size.
            VersionedMessage::V1(solana_message::v1::Message {
                header,
                account_keys: keys,
                lifetime_specifier: blockhash,
                instructions,
                config: solana_message::v1::TransactionConfig::empty(),
            })
        }
    };
    Some(VersionedTransaction { signatures: sigs, message })
}

/// Duplicate-account check that both sanitizers DEFER to `validate_account_locks`
/// (transaction-view/src/sanitize.rs:122; accounts-db/src/account_locks.rs:149).
/// The legacy v1 `validate()` happens to reject duplicates inline, but the
/// banking/replay *acceptance* decision is identical once this deferred check
/// runs — so we fold it into both sides to avoid flagging that by-design
/// asymmetry. NOTE: phase 1 checks STATIC keys only; ALT-resolved duplicates
/// (v0) are a phase-3 concern (needs resolution).
fn has_duplicate(keys: &[Pubkey]) -> bool {
    keys.iter().enumerate().any(|(i, k)| keys[..i].contains(k))
}

/// KNOWN-divergence carve-out (DOCUMENTED). For v1, the VIEW rejects, at parse,
/// a `TransactionConfigMask` with unknown bits (>= bit 5) or a half-set
/// priority-fee pair (bit0 != bit1), while replay's wincode decoder accepts and
/// silently normalizes it (see transaction-view/tests/v1_config_mask_differential.rs
/// and runtime-transaction/tests/v1_mask_divergence.rs). Returns false for such
/// non-canonical v1 masks so the harness folds the VIEW's stricter rule into the
/// legacy side — letting the fuzzer keep hunting for NOVEL divergences instead of
/// re-reporting this one (same philosophy as `has_duplicate`).
/// v1 iff the first wire byte has its MSB set (legacy/v0 begin with a small
/// compact-u16 signature count, MSB clear). Mask is wire[4..8] = version(1)+header(3).
fn v1_mask_is_canonical(wire: &[u8]) -> bool {
    if wire.first().map_or(true, |b| b & 0x80 == 0) || wire.len() < 8 {
        return true; // not v1 (or too short to hold a mask) -> rule N/A
    }
    let mask = u32::from_le_bytes([wire[4], wire[5], wire[6], wire[7]]);
    const ALLOWED: u32 = 0b1_1111;
    let unknown_bits = mask & !ALLOWED != 0;
    let half_priority = (mask & 0b1) != ((mask >> 1) & 0b1);
    !unknown_bits && !half_priority
}

// ---------------------------------------------------------------------------
// PHASE 2 — static metadata parity from the PRODUCTION RuntimeTransaction
// constructors (resolution-independent, so it needs no ALT loader):
//   VIEW:   RuntimeTransaction::<&SanitizedTransactionView>::try_new
//   LEGACY: RuntimeTransaction::<SanitizedVersionedTransaction>::try_from
// For mutually-accepted txs we compare message_hash, is_simple_vote, signature
// details, and compute-budget config — divergences the structural pass can't
// see (e.g. a hash over different bytes, or a compute-budget parse mismatch).
// The known v1 config-mask finding is carved out upstream, so non-canonical
// masks never reach here.
// ---------------------------------------------------------------------------

static FEATURES: LazyLock<FeatureSet> = LazyLock::new(FeatureSet::all_enabled);

#[derive(Debug, PartialEq, Eq)]
struct Meta {
    message_hash: Hash,
    is_simple_vote: bool,
    num_tx_signatures: u64,
    num_secp256k1: u64,
    num_ed25519: u64,
    num_secp256r1: u64,
    instruction_data_len: u16,
    // (heap, cu_limit, priority_fee, loaded_accounts_size); Err if config invalid.
    config: Result<(u32, u32, u64, u32), ()>,
}

fn meta_of(rt: &impl TransactionMeta) -> Meta {
    let sd = rt.signature_details();
    Meta {
        message_hash: *rt.message_hash(),
        is_simple_vote: rt.is_simple_vote_transaction(),
        num_tx_signatures: sd.num_transaction_signatures(),
        num_secp256k1: sd.num_secp256k1_instruction_signatures(),
        num_ed25519: sd.num_ed25519_instruction_signatures(),
        num_secp256r1: sd.num_secp256r1_instruction_signatures(),
        instruction_data_len: rt.instruction_data_len(),
        config: rt
            .transaction_configuration(&FEATURES)
            .map(|c| {
                (
                    c.updated_heap_bytes,
                    c.compute_unit_limit,
                    c.priority_fee_lamports,
                    c.loaded_accounts_data_size_limit,
                )
            })
            .map_err(|_| ()),
    }
}

fn view_meta(v: &SanitizedTransactionView<&[u8]>) -> Option<Meta> {
    // Disambiguate the by-reference constructor (vs the owned / resolved impls).
    RuntimeTransaction::<&SanitizedTransactionView<&[u8]>>::try_new(v, MessageHash::Compute, None)
        .ok()
        .map(|rt| meta_of(&rt))
}

fn legacy_meta(tx: &VersionedTransaction) -> Option<Meta> {
    let svt = SanitizedVersionedTransaction::try_new(tx.clone()).ok()?;
    let rt = RuntimeTransaction::<SanitizedVersionedTransaction>::try_from(
        svt,
        MessageHash::Compute,
        None,
    )
    .ok()?;
    Some(meta_of(&rt))
}

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        let mut u = Unstructured::new(data);
        let enable_ix_acct_limit = u.arbitrary().unwrap_or(true);
        let Ok(recipe) = Recipe::arbitrary(&mut u) else {
            return;
        };
        let Some(tx) = build_tx(&recipe) else { return };

        // Canonical wire bytes. wincode is the serializer the view parser and
        // the runtime agree on (it emits the txv1 custom format for V1).
        let Ok(mut wire) = wincode::serialize(&tx) else {
            return;
        };

        // PARSER-LEVEL mutation: for v1, optionally overwrite the 4-byte
        // TransactionConfigMask (wire[4..8] = version(1)+header(3)) with an
        // arbitrary value. A leader writes block bytes directly, so replay sees
        // attacker-chosen masks the honest serializer would never emit.
        if matches!(tx.version(), SdkVersion::Number(1))
            && recipe.mutate_v1_mask
            && wire.len() >= 8
        {
            wire[4..8].copy_from_slice(&recipe.raw_v1_mask.to_le_bytes());
        }

        // LEGACY (replay) decodes the wire with wincode — ledger replay decodes
        // entries via `wincode::deserialize::<Vec<Entry>>` (ledger/src/shredder.rs).
        // We mirror that here (instead of reusing the pre-built struct) so the
        // harness fuzzes the PARSER as well as the sanitizer.
        let legacy_tx = wincode::deserialize::<VersionedTransaction>(&wire).ok();

        // Consensus-equivalent ACCEPTANCE = sanitize AND the deferred
        // validate_account_locks dup check (folded into BOTH sides), AND the
        // VIEW's stricter v1 config-mask rule folded into the legacy side (known
        // finding; see `v1_mask_is_canonical`).
        let view = view_sanitize(&wire, enable_ix_acct_limit);
        let view_accepts = match &view {
            Some(v) => !has_duplicate(v.static_account_keys()),
            None => false,
        };
        let legacy_accepts = match &legacy_tx {
            Some(t) => {
                legacy_sanitize(t, wire.len(), enable_ix_acct_limit)
                    && !has_duplicate(t.message.static_account_keys())
                    && v1_mask_is_canonical(&wire)
            }
            None => false,
        };

        match (view_accepts, legacy_accepts) {
            // Both accept -> must decode identically.
            (true, true) => {
                let nv = normalize_view(view.as_ref().unwrap());
                let nl = normalize_legacy(legacy_tx.as_ref().unwrap());
                assert_eq!(
                    nv, nl,
                    "STRUCTURAL DIVERGENCE (both accepted)\nflag={enable_ix_acct_limit}\nwire={wire:02x?}"
                );
                // PHASE 2: static metadata parity from the production
                // RuntimeTransaction constructors (message hash, is-simple-vote,
                // signature details, compute-budget config).
                assert_eq!(
                    view_meta(view.as_ref().unwrap()),
                    legacy_meta(legacy_tx.as_ref().unwrap()),
                    "METADATA DIVERGENCE (both accepted)\nflag={enable_ix_acct_limit}\nwire={wire:02x?}"
                );
            }
            // Both reject -> fine.
            (false, false) => {}
            // Accept/reject disagreement -> the fork-class bug we're hunting.
            (true, false) => panic!(
                "DIVERGENCE: VIEW accepted, LEGACY rejected\nflag={enable_ix_acct_limit}\nwire={wire:02x?}"
            ),
            (false, true) => panic!(
                "DIVERGENCE: LEGACY accepted, VIEW rejected\nflag={enable_ix_acct_limit}\nwire={wire:02x?}"
            ),
        }
    });
}

// ===========================================================================
// PHASE 2 — full RuntimeTransaction differential (closer to consensus):
//   VIEW:   RuntimeTransaction::<SanitizedTransactionView<_>>::try_new(view, hash, is_vote)
//   LEGACY: RuntimeTransaction::try_create(tx, hash, is_vote, reserved_keys, flag)
//   Compare: message_hash, is_simple_vote, signature_details, compute-budget /
//   TransactionConfig (heap, CU limit, loaded-accounts-size), writable mask.
//   Deps: solana-runtime-transaction, agave-reserved-account-keys, agave-feature-set.
//
// PHASE 3 — resolved-address (ALT) differential:
//   Provide a deterministic in-memory lookup table keyed by the ATL pubkey
//   (derive its addresses from the fuzz input). Resolve on BOTH sides:
//     VIEW:   ResolvedTransactionView::try_new(view, loaded_addresses, ...)
//     LEGACY: SanitizedMessage::try_new(.., load via the same mock loader)
//   Then assert identical full account-key ordering + per-index writability,
//   and run validate_account_locks on both (duplicate / lock-limit parity).
// ===========================================================================
