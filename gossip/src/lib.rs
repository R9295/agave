#![cfg(feature = "agave-unstable-api")]
#![cfg_attr(feature = "frozen-abi", feature(min_specialization))]
#![allow(clippy::arithmetic_side_effects)]

/// Re-exports of crate-internal wire types so the out-of-tree `fuzz/` harness
/// can round-trip them through wincode. Gated on the `fuzz` feature; not a
/// stable public API.
#[cfg(feature = "fuzz")]
pub mod fuzz {
    pub use crate::{
        contact_info::SocketEntry,
        crds_gossip_pull::CrdsFilter,
        protocol::{Protocol, PruneData},
        restart_crds_values::SlotsOffsets,
        tlv::TlvRecord,
    };
}

pub mod cluster_info;
pub mod cluster_info_metrics;
pub mod contact_info;
pub mod contact_info_notifier;
pub mod crds;
pub mod crds_data;
pub mod crds_entry;
mod crds_filter;
pub mod crds_gossip;
pub mod crds_gossip_error;
pub mod crds_gossip_pull;
pub mod crds_gossip_push;
pub mod crds_shards;
pub mod crds_value;
pub mod duplicate_shred;
pub mod duplicate_shred_handler;
pub mod duplicate_shred_listener;
pub mod epoch_slots;
pub mod epoch_specs;
pub mod gossip_error;
pub mod gossip_service;
pub mod node;
#[macro_use]
mod tlv;
pub mod ping_pong;
mod protocol;
mod push_active_set;
mod received_cache;
pub mod restart_crds_values;
pub mod weighted_shuffle;

#[macro_use]
extern crate log;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

#[cfg_attr(feature = "frozen-abi", macro_use)]
#[cfg(feature = "frozen-abi")]
extern crate solana_frozen_abi_macro;

#[macro_use]
extern crate solana_metrics;

#[cfg(feature = "conformance")]
pub use protocol::gossip_decode_to_effects;

#[cfg(feature = "conformance")]
mod harness;

mod wire_format_tests;
