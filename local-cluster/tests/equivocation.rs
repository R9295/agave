#![cfg(feature = "dev-context-only-utils")]

use {
    agave_votor_messages::{
        certificate::CertificateType,
        consensus_message::BlockId,
        unverified_vote_message::DecodedWireConsensusMessage,
        wire::{VersionedWireConsensusMessage, get_vote_payload_to_sign},
    },
    arc_swap::ArcSwap,
    crossbeam_channel::{bounded, unbounded},
    serial_test::serial,
    solana_account::AccountSharedData,
    solana_bls_signatures::{VerifySignature, signature::SignatureAffine},
    solana_core::validator::{AlternateBlockPlan, EquivocationResult, ValidatorConfig},
    solana_entry::{
        block_component::{VersionedBlockFooter, VersionedBlockMarker},
        entry::Entry,
    },
    solana_keypair::Keypair,
    solana_leader_schedule::FixedSchedule,
    solana_ledger::{
        blockstore::Blockstore,
        blockstore_meta::BlockLocation,
        shred::{
            Shred,
            filter::{TurbineMode, TurbineModeKind},
        },
    },
    solana_local_cluster::{
        cluster_tests::start_datagram_listener_for_alpenglow_votor,
        integration_tests::{DEFAULT_NODE_STAKE, create_custom_leader_schedule_with_random_keys},
        local_cluster::{ClusterConfig, LocalCluster},
        validator_configs::make_identical_validator_configs,
    },
    solana_net_utils::{SocketAddrSpace, sockets::bind_to_localhost_unique},
    solana_perf::packet::packet_config,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_system_interface::program as system_program,
    std::{
        collections::{BTreeMap, HashMap, HashSet},
        sync::Arc,
        time::{Duration, Instant},
    },
};

/// Read a particular version without changing the live validator's selected block.
fn read_block(
    blockstore: &Blockstore,
    block_id: BlockId,
    slot: u64,
    leader: &Pubkey,
) -> (Vec<Entry>, VersionedBlockFooter) {
    let (meta, _) = blockstore
        .get_slot_meta_for_block_id(slot, block_id.to_hash())
        .unwrap()
        .unwrap();
    let shreds: Vec<_> = (0..=meta.last_index.unwrap())
        .map(|index| {
            let bytes = blockstore
                .get_data_shred_for_block_id(slot, index, block_id.to_hash())
                .unwrap()
                .unwrap();
            let shred = Shred::new_from_serialized_shred(bytes).unwrap();
            assert!(shred.verify(leader));
            shred
        })
        .collect();
    let directory = tempfile::tempdir().unwrap();
    let copy = Blockstore::open(directory.path()).unwrap();
    copy.insert_shreds(shreds, false).unwrap();
    let (components, _, full) = copy
        .get_slot_components_with_shred_info(slot, 0, false)
        .unwrap();
    assert!(full);
    let footer = components
        .iter()
        .find_map(|component| {
            let VersionedBlockMarker::V1(marker) = component.as_marker()?;
            marker.as_block_footer().cloned()
        })
        .expect("complete block has a footer");
    (copy.get_slot_entries(slot, 0).unwrap(), footer)
}

/// One malicious leader retains A and broadcasts a valid, independently executed B.
/// Four honest validators receive B over Turbine; a fifth disables Turbine and
/// repairs only from the malicious leader. Honest stake can finalize B without
/// the malicious leader's vote. B is alternate to A, not permanently noncanonical:
/// this test requires it to become the rooted version.
#[test]
#[serial]
fn test_alpenglow_equivocation_broadcast_repair_vote_and_finalize() {
    agave_logger::setup_with_default(
        "error,equivocation=info,solana_turbine::broadcast_stage::equivocation=info,\
         solana_core::replay_stage=warn",
    );
    const NUM_NODES: usize = 6;
    // Last slot of a leader window, with ample time for gossip and vote connections.
    const SLOT: u64 = 63;
    const TRANSFER: u64 = 1_000_000_000;
    let (schedule, keys) = create_custom_leader_schedule_with_random_keys(&[64, 4, 4, 4, 4, 4]);
    let identities: Vec<_> = keys.iter().map(|keys| keys.node_keypair.pubkey()).collect();
    let payer = Arc::new(Keypair::new());
    let recipient = Pubkey::new_unique();
    let (produced_sender, produced_receiver) = bounded(1);

    // Observe authenticated network traffic; do not inject votes or certificates.
    let listener_keypair = Keypair::new();
    let listener_identity = listener_keypair.pubkey();
    let listener_socket = bind_to_localhost_unique().unwrap();
    let listener_addr = listener_socket.local_addr().unwrap();
    let (runtime, listener_datagrams, endpoint) =
        start_datagram_listener_for_alpenglow_votor(listener_socket, listener_keypair, &identities);
    // The listener drops datagrams once its bounded channel is full, and
    // LocalCluster::new can run past the target slot. Drain it from before
    // cluster startup so target-slot votes and certificates are retained.
    let (datagram_sender, datagrams) = unbounded();
    let forwarder = std::thread::spawn(move || {
        for datagram in listener_datagrams {
            if datagram_sender.send(datagram).is_err() {
                break;
            }
        }
    });

    let mut base = ValidatorConfig::default_for_test();
    base.wait_for_supermajority = Some(0);
    base.fixed_leader_schedule = Some(FixedSchedule {
        leader_schedule: Arc::new(schedule),
    });
    base.votor_peer_overrides = Arc::new(ArcSwap::from_pointee(HashMap::from([(
        listener_identity,
        Some(listener_addr),
    )])));
    let mut configs = make_identical_validator_configs(&base, NUM_NODES);
    configs[0].equivocation_schedule = BTreeMap::from([(
        SLOT,
        AlternateBlockPlan {
            payer: payer.clone(),
            recipient,
            lamports: TRANSFER,
            recipients: identities[1..5].to_vec(),
            original_recipients: Some(vec![]),
            produced_sender: Some(produced_sender),
        },
    )]);
    // B's broadcast recipients cannot repair the target slot, so their B votes
    // prove broadcast delivery and eager repair cannot fetch the leader's A
    // first. Startup catch-up and every other slot use normal repair.
    for config in &mut configs[1..5] {
        config.turbine_mode = TurbineMode::for_slots(TurbineModeKind::RepairDisabled, SLOT..=SLOT);
    }
    // This witness cannot obtain B from broadcast or repair from an honest peer.
    configs[5].turbine_mode = TurbineMode::new(TurbineModeKind::TurbineDisabled);
    configs[5].repair_validators = Some(HashSet::from([identities[0]]));
    let mut config = ClusterConfig {
        validator_configs: configs,
        validator_keys: Some(keys.into_iter().map(|keys| (keys, true)).collect()),
        node_stakes: vec![DEFAULT_NODE_STAKE; NUM_NODES],
        slots_per_epoch: 512,
        stakers_slot_offset: 512,
        skip_warmup_slots: true,
        additional_accounts: vec![(
            payer.pubkey(),
            AccountSharedData::empty(10 * TRANSFER, &system_program::id()),
        )],
        ..ClusterConfig::default()
    };
    let cluster = LocalCluster::new(&mut config, SocketAddrSpace::Unspecified);
    let validators: Vec<_> = identities
        .iter()
        .map(|id| cluster.validators[id].validator.as_ref().unwrap())
        .collect();
    let shred_version =
        solana_shred_version::compute_shred_version(&cluster.genesis_config.hash(), None);
    let observation_bank = validators[1].bank_forks.read().unwrap().root_bank();
    let rank_map = observation_bank.get_rank_map(SLOT).unwrap();
    let mut produced: Option<EquivocationResult> = None;
    let mut notar_votes = HashMap::<Pubkey, HashSet<BlockId>>::new();
    let mut finalization_certificates = Vec::new();
    let start = Instant::now();
    let mut last_report = Instant::now();

    loop {
        if produced.is_none() {
            produced = produced_receiver.try_recv().ok();
            if let Some(result) = produced {
                assert_eq!(result.slot, SLOT);
                assert_ne!(result.original_block_id, result.alternate_block_id);
                assert_ne!(result.original_bank_hash, result.alternate_bank_hash);
                log::info!("produced A/B: {result:?}");
            }
        }
        if let Ok(datagram) = datagrams.recv_timeout(Duration::from_millis(50)) {
            let wire = VersionedWireConsensusMessage::deserialize_with_expected_shred_version(
                &datagram.message[..],
                packet_config(),
                shred_version,
            )
            .unwrap();
            match DecodedWireConsensusMessage::new(wire) {
                DecodedWireConsensusMessage::Vote(message) if message.vote.slot() == SLOT => {
                    let (_, entry) = rank_map
                        .get_ranked_entry_for_node(&datagram.peer_pubkey)
                        .unwrap();
                    entry
                        .bls_pubkey
                        .verify_signature(
                            &SignatureAffine::try_from(message.signature).unwrap(),
                            &get_vote_payload_to_sign(message.vote, shred_version),
                        )
                        .unwrap();
                    if message.vote.is_notarization() {
                        notar_votes
                            .entry(datagram.peer_pubkey)
                            .or_default()
                            .insert(*message.vote.block_id().unwrap());
                        log::info!(
                            "observed notarization from {}: {:?}",
                            datagram.peer_pubkey,
                            message.vote
                        );
                    }
                }
                DecodedWireConsensusMessage::Certificate(cert)
                    if cert.cert_type.slot() == SLOT && cert.cert_type.is_finalization() =>
                {
                    finalization_certificates.push(cert.cert_type);
                }
                _ => (),
            }
        }
        let roots: Vec<_> = validators
            .iter()
            .map(|v| v.bank_forks.read().unwrap().root())
            .collect();
        if let Some(result) = produced {
            let b_id = BlockId::from(result.alternate_block_id);
            let honest_b_voters = identities[1..]
                .iter()
                .filter(|id| {
                    notar_votes
                        .get(id)
                        .is_some_and(|votes| votes.contains(&b_id))
                })
                .count();
            let b_finalized = finalization_certificates.iter().any(|cert| match cert {
                CertificateType::Finalize(slot) => *slot == SLOT,
                CertificateType::FinalizeFast(block) => {
                    block.slot == SLOT && block.block_id == b_id
                }
                _ => false,
            });
            let honest_roots_b = validators[1..].iter().all(|v| {
                v.blockstore.is_root(SLOT)
                    && v.blockstore
                        .get_double_merkle_root(SLOT, BlockLocation::Original)
                        .unwrap()
                        == Some(result.alternate_block_id)
            });
            if honest_b_voters >= 3 && b_finalized && honest_roots_b {
                for validator in &validators[1..] {
                    // Live replay does not populate the BankHash column; it
                    // verifies the computed hash against the footer before
                    // freezing. Check each honest validator's own persisted B.
                    assert!(!validator.blockstore.is_dead(SLOT));
                    let (_, VersionedBlockFooter::V1(footer)) =
                        read_block(&validator.blockstore, b_id, SLOT, &identities[0]);
                    assert_eq!(footer.bank_hash, result.alternate_bank_hash);
                    assert_eq!(
                        validator
                            .bank_forks
                            .read()
                            .unwrap()
                            .root_bank()
                            .get_balance(&recipient),
                        TRANSFER
                    );
                }
                // The malicious leader retains both versions, even if normal fork
                // selection has since promoted B and backed up A.
                let (entries_a, VersionedBlockFooter::V1(footer_a)) = read_block(
                    &validators[0].blockstore,
                    result.original_block_id.into(),
                    SLOT,
                    &identities[0],
                );
                let (entries_b, VersionedBlockFooter::V1(footer_b)) =
                    read_block(&validators[0].blockstore, b_id, SLOT, &identities[0]);
                assert_eq!(footer_a.bank_hash, result.original_bank_hash);
                assert_eq!(footer_b.bank_hash, result.alternate_bank_hash);
                let txs_a: Vec<_> = entries_a.into_iter().flat_map(|e| e.transactions).collect();
                let txs_b: Vec<_> = entries_b.into_iter().flat_map(|e| e.transactions).collect();
                assert_ne!(txs_a, txs_b);
                assert_eq!(txs_b.len(), 1);
                let expected = solana_system_transaction::transfer(
                    &payer,
                    &recipient,
                    TRANSFER,
                    *txs_b[0].message.recent_blockhash(),
                );
                assert_eq!(txs_b[0], expected.into());
                log::info!(
                    "PASS: B broadcast to witness, repaired from malicious leader, notarized by \
                     {honest_b_voters} honest validators, and rooted by all five; roots={roots:?}"
                );
                break;
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "equivocation did not complete: produced={produced:?}, votes={notar_votes:?}, \
             certificates={finalization_certificates:?}, roots={roots:?}"
        );
        if last_report.elapsed() >= Duration::from_secs(5) {
            log::info!(
                "waiting for equivocation: produced={produced:?}, votes={notar_votes:?}, \
                 certificates={finalization_certificates:?}, roots={roots:?}"
            );
            last_report = Instant::now();
        }
    }
    // Stop observing: dropping the endpoint and its runtime closes the
    // listener channel, which ends the forwarder.
    drop(datagrams);
    drop(endpoint);
    drop(runtime);
    forwarder.join().unwrap();
}
