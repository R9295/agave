#![no_main]

//! Bidirectional wincode roundtrip fuzzing for types that implement
//! `SchemaRead`/`SchemaWrite` manually or via a `#[wincode(with = ...)]`
//! custom helper.

use libfuzzer_sys::fuzz_target;


/// For any input that deserializes, assert that re-serializing reproduces the
/// exact original bytes.
macro_rules! fuzz_roundtrip {
    ($data:expr, $ty:ty) => {
        if let Ok(value) = wincode::deserialize_exact::<$ty>($data) {
            let serialized = wincode::serialize(&value).expect("serialize should succeed");
            assert_eq!(
                $data, serialized,
                "deserialize -> serialize != orignal_data for type {}\nserialized {:?}\noriginal   {:?}",
                stringify!($ty), serialized, $data,
            );
        }
    };
}

/// For any input that deserializes, assert that re-serializing reproduces the
/// exact original bytes, and that the value survives a full roundtrip.
macro_rules! fuzz_roundtrip_deserialize {
    ($data:expr, $ty:ty) => {
        if let Ok(value) = wincode::deserialize_exact::<$ty>($data) {
            let serialized = wincode::serialize(&value).expect("serialize should succeed");
            assert_eq!(
                $data, serialized,
                "deserialize -> serialize != orignal_data for type {}\nserialized {:?}\noriginal   {:?}",
                stringify!($ty), serialized, $data,
            );
            let roundtrip: $ty =
                wincode::deserialize_exact(&serialized).expect("roundtrip deserialize should succeed");
            assert_eq!(value, roundtrip, "roundtrip failed for {}", stringify!($ty));
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, payload)) = data.split_first() else {
        return;
    };

    match selector {
        // solana-ledger
        0 => fuzz_roundtrip_deserialize!(payload, solana_ledger::shred::ShredVariant),
        1 => fuzz_roundtrip_deserialize!(payload, solana_ledger::shred::DataShredHeader),
        2 => fuzz_roundtrip_deserialize!(payload, solana_ledger::blockstore_meta::SlotMetaV3),
        3 => fuzz_roundtrip_deserialize!(payload, solana_ledger::blockstore_meta::ErasureMeta),
        // solana-entry
        4 => fuzz_roundtrip_deserialize!(payload, solana_entry::block_component::BlockComponent),
        5 => fuzz_roundtrip_deserialize!(payload, solana_entry::block_component::BlockFooterV1),
        6 => fuzz_roundtrip_deserialize!(payload, solana_entry::block_component::GenesisCertBlockMarker),
        7 => fuzz_roundtrip_deserialize!(payload, solana_entry::block_component::VotesAggregate),
        8 => fuzz_roundtrip_deserialize!(payload, solana_entry::entry::Entry),
        // solana-gossip
        9 => fuzz_roundtrip_deserialize!(payload, solana_gossip::contact_info::ContactInfo),
        10 => fuzz_roundtrip_deserialize!(payload, solana_gossip::contact_info::SocketEntry),
        11 => fuzz_roundtrip_deserialize!(payload, solana_gossip::crds_value::CrdsValue),
        12 => fuzz_roundtrip_deserialize!(payload, solana_gossip::crds_data::CrdsData),
        13 => fuzz_roundtrip_deserialize!(payload, solana_gossip::crds_data::Vote),
        14 => fuzz_roundtrip_deserialize!(payload, solana_gossip::tlv::TlvRecord),
        // solana-version
        15 => fuzz_roundtrip_deserialize!(payload, solana_version::Version),
        // solana-storage-proto
        // We do not re-deserialize and compare due to lacking Eq impl.
        16 => fuzz_roundtrip!(payload, solana_storage_proto::StoredExtendedReward),
        17 => fuzz_roundtrip!(payload, solana_storage_proto::StoredTransactionTokenBalance),
        18 => fuzz_roundtrip!(payload, solana_storage_proto::StoredTransactionStatusMeta),
        // agave-votor-messages
        19 => fuzz_roundtrip_deserialize!(payload, agave_votor_messages::reward_certificate::SkipRewardCertificate),
        20 => fuzz_roundtrip_deserialize!(payload, agave_votor_messages::reward_certificate::NotarRewardCertificate),
        21 => fuzz_roundtrip_deserialize!(payload, agave_votor_messages::wire::WireCertSignature),
        22 => fuzz_roundtrip_deserialize!(payload, agave_votor_messages::wire::WireVoteSignature),
        _ => {}
    }
});
