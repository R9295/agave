use wincode::config::DefaultConfig;

/// Reachable types implementing `SchemaWrite` + `SchemaRead` as an owned
/// (`Src = Dst = Self`) roundtrip type. Generic types (`Bloom<T>`,
/// `LengthPrefixed<T>`, `Ping<N>`, `BitVec<N>`, `SlotMetaBase<T>`), read-only
/// reference types, and the `votor` history types (whose `HashMap`/`HashSet`
/// fields have non-deterministic encode order) are intentionally excluded.
static TARGETS: &[Target] = &[
    // ---- agave-votor-messages -------------------------------------------
    roundtrip::<agave_votor_messages::vote::Vote>,
    roundtrip::<agave_votor_messages::vote::NotarizationVote>,
    roundtrip::<agave_votor_messages::vote::FinalizationVote>,
    roundtrip::<agave_votor_messages::vote::SkipVote>,
    roundtrip::<agave_votor_messages::vote::NotarizationFallbackVote>,
    roundtrip::<agave_votor_messages::vote::SkipFallbackVote>,
    roundtrip::<agave_votor_messages::vote::GenesisVote>,
    roundtrip::<agave_votor_messages::consensus_message::Block>,
    roundtrip::<agave_votor_messages::consensus_message::VoteMessage>,
    roundtrip::<agave_votor_messages::consensus_message::ConsensusMessage>,
    roundtrip::<agave_votor_messages::certificate::Certificate>,
    roundtrip::<agave_votor_messages::certificate::CertificateType>,
    roundtrip::<agave_votor_messages::reward_certificate::SkipRewardCertificate>,
    roundtrip::<agave_votor_messages::reward_certificate::NotarRewardCertificate>,
    // ---- solana-entry ---------------------------------------------------
    roundtrip::<solana_entry::entry::Entry>,
    roundtrip::<solana_entry::block_component::BlockFooterV1>,
    roundtrip::<solana_entry::block_component::BlockHeaderV1>,
    roundtrip::<solana_entry::block_component::UpdateParentV1>,
    roundtrip::<solana_entry::block_component::GenesisCertBlockMarker>,
    roundtrip::<solana_entry::block_component::BlockFinalizationCert>,
    roundtrip::<solana_entry::block_component::VotesAggregate>,
    roundtrip::<solana_entry::block_component::VersionedBlockFooter>,
    roundtrip::<solana_entry::block_component::VersionedBlockHeader>,
    roundtrip::<solana_entry::block_component::VersionedUpdateParent>,
    roundtrip::<solana_entry::block_component::BlockMarkerV1>,
    roundtrip::<solana_entry::block_component::VersionedBlockMarker>,
    // ---- solana-gossip --------------------------------------------------
    // (CrdsValue / crds_data::Vote / ContactInfo use hand-written SchemaRead.)
    roundtrip::<solana_gossip::crds_value::CrdsValue>,
    roundtrip::<solana_gossip::crds_data::CrdsData>,
    roundtrip::<solana_gossip::crds_data::SnapshotHashes>,
    roundtrip::<solana_gossip::crds_data::LowestSlot>,
    roundtrip::<solana_gossip::crds_data::Vote>,
    roundtrip::<solana_gossip::contact_info::ContactInfo>,
    roundtrip::<solana_gossip::epoch_slots::EpochSlots>,
    roundtrip::<solana_gossip::epoch_slots::Uncompressed>,
    roundtrip::<solana_gossip::epoch_slots::Flate2>,
    roundtrip::<solana_gossip::epoch_slots::CompressedSlots>,
    roundtrip::<solana_gossip::duplicate_shred::DuplicateShred>,
    roundtrip::<solana_gossip::restart_crds_values::RestartLastVotedForkSlots>,
    roundtrip::<solana_gossip::restart_crds_values::RestartHeaviestFork>,
    // ---- solana-ledger --------------------------------------------------
    roundtrip::<solana_ledger::blockstore_meta::CompletedDataIndexes>,
    roundtrip::<solana_ledger::blockstore_meta::SlotMetaV3>,
    roundtrip::<solana_ledger::blockstore_meta::Index>,
    roundtrip::<solana_ledger::blockstore_meta::ErasureMeta>,
    roundtrip::<solana_ledger::blockstore_meta::MerkleRootMeta>,
    roundtrip::<solana_ledger::blockstore_meta::DuplicateSlotProof>,
    roundtrip::<solana_ledger::blockstore_meta::FrozenHashVersioned>,
    roundtrip::<solana_ledger::blockstore_meta::FrozenHashStatus>,
    roundtrip::<solana_ledger::blockstore_meta::ShredIndex>,
    roundtrip::<solana_ledger::blockstore_meta::AddressSignatureMeta>,
    roundtrip::<solana_ledger::blockstore_meta::PerfSample>,
    roundtrip::<solana_ledger::blockstore_meta::OptimisticSlotMetaV0>,
    roundtrip::<solana_ledger::blockstore_meta::OptimisticSlotMetaVersioned>,
    roundtrip::<solana_ledger::blockstore_meta::DoubleMerkleMeta>,
    roundtrip::<solana_ledger::shred::Payload>,
    // ---- solana-storage-proto -------------------------------------------
    roundtrip::<solana_storage_proto::StoredExtendedReward>,
    roundtrip::<solana_storage_proto::StoredTokenAmount>,
    roundtrip::<solana_storage_proto::StoredTransactionTokenBalance>,
    roundtrip::<solana_storage_proto::StoredTransactionStatusMeta>,
    // ---- solana-transaction-status-client-types -------------------------
    roundtrip::<solana_transaction_status_client_types::InnerInstructions>,
    roundtrip::<solana_transaction_status_client_types::InnerInstruction>,
    // ---- solana-transaction-context -------------------------------------
    roundtrip::<solana_transaction_context::transaction::TransactionReturnData>,
    // ---- solana-faucet --------------------------------------------------
    roundtrip::<solana_faucet::faucet::FaucetRequest>,
    // ---- solana-core (repair wire protocol) -----------------------------
    roundtrip::<solana_core::repair::serve_repair::AncestorHashesResponse>,
    roundtrip::<solana_core::repair::serve_repair::BlockIdRepairResponse>,
    roundtrip::<solana_core::repair::serve_repair::RepairRequestHeader>,
    roundtrip::<solana_core::repair::serve_repair::RepairProtocol>,
    // ---- solana-gossip (wire envelope, pull filter, restart, TLV) -------
    roundtrip::<solana_gossip::fuzz::Protocol>,
    roundtrip::<solana_gossip::fuzz::PruneData>,
    roundtrip::<solana_gossip::fuzz::CrdsFilter>,
    roundtrip::<solana_gossip::fuzz::TlvRecord>,
    roundtrip::<solana_gossip::fuzz::SocketEntry>,
    roundtrip::<solana_gossip::fuzz::SlotsOffsets>,
];

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        dispatch(data);
    });
}

/// Select a target with the leading byte and roundtrip the remainder through it.
fn dispatch(data: &[u8]) {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    // Map the selector byte onto a target; the fuzzer explores the byte.
    let target = TARGETS[selector as usize % TARGETS.len()];
    target(rest);
}

fn roundtrip<T>(data: &[u8])
where
    T: wincode::SchemaWrite<DefaultConfig, Src = T>
        + for<'de> wincode::SchemaRead<'de, DefaultConfig, Dst = T>,
{
    // Only well-formed, fully-consumed inputs are interesting; reject the rest.
    let Ok(value) = wincode::deserialize_exact::<T>(data) else {
        return;
    };

    // A value we just decoded must always re-encode.
    let reserialized = wincode::serialize::<T>(&value).unwrap_or_else(|err| {
        panic!(
            "serialize failed for {} on a value produced by deserialize_exact: {err:?}",
            core::any::type_name::<T>(),
        )
    });

    // Canonical codec: re-encoding must reproduce the exact input bytes.
    assert!(
        reserialized == data,
        "non-canonical wincode roundtrip for {}: {}-byte input != {}-byte \
         reserialization\nor={:?}\nre={:?}",
        core::any::type_name::<T>(),
        data.len(),
        reserialized.len(),
        data,
        reserialized,
    );
}

type Target = fn(&[u8]);
