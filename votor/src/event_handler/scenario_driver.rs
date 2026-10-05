//! Synchronous wiring of the production voting loop and consensus pool for scenarios.

use {
    crate::{
        consensus_pool::ConsensusPool,
        consensus_pool_service::{PoolMessage, PoolVote},
        event::{CompletedBlock, LatestSwitchRequest, VotorEvent},
        event_handler::{EventHandler, LocalContext, stats::EventHandlerStats},
        root_utils::RootContext,
        slot_clock::SharedAlpenglowSlotClock,
        timer_manager::TimerManager,
        vote_history::VoteHistory,
        vote_history_storage::NullVoteHistoryStorage,
        voting_service::BLSOp,
        voting_utils::VotingContext,
        votor::SharedContext,
    },
    agave_bls_sigverify::{
        generated_cert_types::GeneratedCertTypes, sig_verified_messages::VoteAggregate,
    },
    agave_votor_messages::{
        certificate::{Certificate, CertificateType},
        consensus_message::{Block, VoteMessage},
        migration::MigrationStatus,
        vote::Vote,
        wire::get_vote_payload_to_sign,
    },
    bitvec::vec::BitVec,
    crossbeam_channel::{Receiver, unbounded},
    parking_lot::RwLock as PlRwLock,
    solana_bls_signatures::{BLS_SIGNATURE_AFFINE_SIZE, Signature as BLSSignature},
    solana_clock::Slot,
    solana_gossip::{cluster_info::ClusterInfo, contact_info::ContactInfo},
    solana_keypair::Keypair,
    solana_ledger::{
        blockstore::Blockstore, blockstore_options::BlockstoreOptions,
        leader_schedule_cache::LeaderScheduleCache,
    },
    solana_net_utils::SocketAddrSpace,
    solana_runtime::{
        bank::{Bank, BankTestConfig, SlotLeader},
        bank_forks::BankForks,
        bank_forks_controller::{BankForksController, BankForksControllerError},
        genesis_utils::{
            ValidatorVoteKeypairs, create_genesis_config_with_alpenglow_vote_accounts,
        },
        installed_scheduler_pool::BankWithScheduler,
        slot_params::slot_time_feature_ids,
    },
    solana_signer::Signer,
    solana_signer_store::{encode_base2, encode_base3},
    solana_streamer::evicting_sender::EvictingSender,
    std::{
        collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
        sync::{Arc, Mutex, RwLock},
        time::Duration,
    },
    tempfile::TempDir,
};

/// Root requests are observable without a replay thread or snapshot machinery.
struct ScenarioBankForksController {
    bank_forks: Arc<RwLock<BankForks>>,
    requested_root: Arc<Mutex<Option<Block>>>,
}

impl BankForksController for ScenarioBankForksController {
    fn insert_bank(&self, bank: Bank) -> Result<BankWithScheduler, BankForksControllerError> {
        Ok(self.bank_forks.write().unwrap().insert(bank))
    }

    fn enqueue_set_root(&self, block: Block) {
        *self.requested_root.lock().unwrap() = Some(block);
    }

    fn clear_bank(&self, slot: Slot) -> Result<(), BankForksControllerError> {
        let mut bank_forks = self.bank_forks.write().unwrap();
        if bank_forks.get(slot).is_some() {
            let _ = bank_forks.remove(slot);
        }
        Ok(())
    }
}

pub(super) struct Driver {
    validators: Vec<ValidatorVoteKeypairs>,
    pool: ConsensusPool,
    shared: SharedContext,
    voting: VotingContext,
    rooting: RootContext,
    local: LocalContext,
    timers: PlRwLock<TimerManager>,
    own_votes: Receiver<VoteMessage>,
    bls_ops: Receiver<BLSOp>,
    drain_auxiliary: Box<dyn FnMut()>,
    requested_root: Arc<Mutex<Option<Block>>>,
    replayed: BTreeMap<Block, Arc<Bank>>,
    pending_safe_to_notar: BTreeSet<Block>,
    certificates: BTreeSet<CertificateType>,
    votes: Vec<(Vote, bool)>,
    // Drop all banks and the blockstore before their temporary directories.
    _directory: TempDir,
}

impl Driver {
    pub(super) fn new() -> Self {
        let validators = (0..20u8)
            .map(|index| {
                ValidatorVoteKeypairs::new(
                    Keypair::new_from_array([index * 3 + 1; 32]),
                    Keypair::new_from_array([index * 3 + 2; 32]),
                    Keypair::new_from_array([index * 3 + 3; 32]),
                )
            })
            .collect::<Vec<_>>();
        let mut genesis = create_genesis_config_with_alpenglow_vote_accounts(
            1_000_000_000,
            &validators,
            (0..20)
                .map(|i| if i < 10 { 620_000 } else { 380_000 })
                .collect(),
        );
        genesis.genesis_config.creation_time = 0;
        // Firedancer scenarios use 400ms slots across every leader window.
        for feature in slot_time_feature_ids() {
            genesis.genesis_config.accounts.remove(&feature);
        }
        let directory = TempDir::new().unwrap();
        let mut bank_config = BankTestConfig::default();
        bank_config.accounts_db_config.bank_hash_details_dir = directory.path().to_path_buf();
        let bank = Bank::new_with_paths_for_tests(
            &genesis.genesis_config,
            Some(bank_config),
            vec![],
            None,
        );
        let genesis_block = Block::default();
        bank.set_block_id(Some(genesis_block.block_id.to_hash()));
        bank.freeze();
        let bank_forks = BankForks::new_rw_arc(bank);
        let bank = bank_forks.read().unwrap().root_bank();
        let node_keypair = Arc::new(validators[0].node_keypair.insecure_clone());
        let vote_keypair = Arc::new(validators[0].vote_keypair.insecure_clone());
        let cluster_info = Arc::new(ClusterInfo::new(
            ContactInfo::new_localhost(&node_keypair.pubkey(), 0),
            node_keypair.clone(),
            SocketAddrSpace::Unspecified,
        ));
        let blockstore = Arc::new(
            Blockstore::open_with_options(
                &directory.path().join("ledger"),
                BlockstoreOptions::default_for_tests(),
            )
            .unwrap(),
        );
        let leader_schedule = Arc::new(LeaderScheduleCache::new_from_bank(&bank));
        let (bls_sender, bls_ops) = unbounded();
        let (commitment_sender, commitments) = unbounded();
        let (own_vote_sender, own_votes) = EvictingSender::new_bounded(1024);
        let (own_reward_sender, rewards) = unbounded();
        let (consensus_metrics_sender, metrics) = unbounded();
        let (leader_window_info_sender, windows) = unbounded();
        let (repair_event_sender, repairs) = unbounded();
        let vote_history_storage = Arc::new(NullVoteHistoryStorage::default());
        let shared = SharedContext {
            cluster_info: cluster_info.clone(),
            alpenglow_slot_clock: SharedAlpenglowSlotClock::default(),
            bank_forks: bank_forks.clone(),
            vote_history_storage: vote_history_storage.clone(),
            leader_window_info_sender,
            blockstore,
            highest_parent_ready: Arc::new(RwLock::default()),
            repair_event_sender,
            latest_switch_request: LatestSwitchRequest::default(),
        };
        let mut vote_history = VoteHistory::new(node_keypair.pubkey(), 0);
        vote_history.initialize_genesis(genesis_block);
        let voting = VotingContext {
            cluster_info: cluster_info.clone(),
            identity_keypair: node_keypair,
            sharable_banks: bank_forks.read().unwrap().sharable_banks(),
            vote_history,
            bls_sender,
            commitment_sender,
            vote_account_pubkey: vote_keypair.pubkey(),
            wait_to_vote_slot: None,
            authorized_voter_keypairs: Arc::new(RwLock::new(vec![vote_keypair])),
            vote_history_storage,
            derived_bls_keypairs: HashMap::new(),
            own_vote_sender,
            own_reward_sender,
            consensus_metrics_sender,
            leader_schedule,
        };
        let requested_root = Arc::new(Mutex::new(None));
        let rooting = RootContext {
            bank_notification_sender: None,
            bank_forks_controller: Arc::new(ScenarioBankForksController {
                bank_forks,
                requested_root: requested_root.clone(),
            }),
        };
        let local = LocalContext {
            my_pubkey: cluster_info.id(),
            genesis_block,
            pending_blocks: BTreeMap::new(),
            finalized_blocks: BTreeSet::new(),
            received_shred: BTreeSet::new(),
            stats: EventHandlerStats::default(),
            standstill_slot: None,
        };
        let pool = ConsensusPool::new(
            cluster_info,
            &bank,
            Arc::new(GeneratedCertTypes::default()),
            Arc::new(MigrationStatus::post_migration_status()),
            (1, genesis_block),
        );
        let mut driver = Self {
            validators,
            pool,
            shared,
            voting,
            rooting,
            local,
            timers: PlRwLock::new(TimerManager::new_for_scenarios()),
            own_votes,
            bls_ops,
            drain_auxiliary: Box::new(move || {
                commitments.try_iter().for_each(drop);
                rewards.try_iter().for_each(drop);
                metrics.try_iter().for_each(drop);
                windows.try_iter().for_each(drop);
                repairs.try_iter().for_each(drop);
            }),
            requested_root,
            replayed: BTreeMap::from([(genesis_block, bank)]),
            pending_safe_to_notar: BTreeSet::new(),
            certificates: BTreeSet::new(),
            votes: vec![],
            _directory: directory,
        };
        driver.event(VotorEvent::ParentReady {
            slot: 1,
            parent_block: genesis_block,
        });
        driver
    }

    pub(super) fn external_vote(&self, index: usize, vote: Vote) -> PoolVote {
        let bank = self.bank();
        let ranks = bank.get_rank_map(vote.slot()).unwrap();
        let rank = *ranks
            .get_rank_for_vote_pubkey(&self.validators[index].vote_keypair.pubkey())
            .unwrap();
        PoolVote::External(VoteAggregate::new_from_verified_vote(
            ranks.len(),
            VoteMessage {
                vote,
                signature: self.validators[index]
                    .bls_keypair
                    .sign(&get_vote_payload_to_sign(
                        vote,
                        self.shared.cluster_info.my_shred_version(),
                    ))
                    .into(),
                rank,
                stake: ranks.get_pubkey_stake_entry(rank as usize).unwrap().stake,
            },
        ))
    }

    pub(super) fn certificate(&self, cert_type: CertificateType) -> Certificate {
        let bank = self.bank();
        let ranks = bank.get_rank_map(cert_type.slot()).unwrap();
        let mut base = BitVec::repeat(false, ranks.len());
        let mut fallback = BitVec::repeat(false, ranks.len());
        let end = if cert_type.is_fast_finalization() {
            16
        } else {
            11
        };
        for index in 1..=end {
            let rank = *ranks
                .get_rank_for_vote_pubkey(&self.validators[index].vote_keypair.pubkey())
                .unwrap() as usize;
            if cert_type.is_notarize_fallback() && index >= 6 {
                fallback.set(rank, true);
            } else {
                base.set(rank, true);
            }
        }
        let bitmap = if cert_type.is_notarize_fallback() {
            encode_base3(&base, &fallback)
        } else {
            encode_base2(&base)
        }
        .unwrap();
        Certificate {
            cert_type,
            signature: BLSSignature([0; BLS_SIGNATURE_AFFINE_SIZE]),
            bitmap,
        }
    }

    pub(super) fn bank(&self) -> Arc<Bank> {
        self.voting.sharable_banks.root()
    }

    pub(super) fn pool_message(&mut self, message: PoolMessage) {
        let mut events = VecDeque::new();
        self.ingest(message, &mut events);
        self.pump(events);
    }

    pub(super) fn event(&mut self, event: VotorEvent) {
        self.pump(VecDeque::from([event]));
    }

    pub(super) fn advance_clock(&mut self, elapsed: Duration) {
        let events = self.timers.write().advance_clock(elapsed);
        self.pump(events.into());
    }

    pub(super) fn replay(&mut self, block: Block, parent: Block) -> bool {
        if self.replayed.contains_key(&block) {
            return true;
        }
        if block.slot <= parent.slot || block.slot <= self.vote_root() {
            return false;
        }
        let Some(parent_bank) = self.replayed.get(&parent).cloned() else {
            return false;
        };
        // Bank creation reads the fork graph through its program cache.
        let bank = Bank::new_from_parent(parent_bank, SlotLeader::default(), block.slot);
        bank.set_block_id(Some(block.block_id.to_hash()));
        bank.freeze();
        let mut bank_forks = self.shared.bank_forks.write().unwrap();
        if bank_forks.get(block.slot).is_some() {
            // Keep earlier frozen versions alive for the scenario's fork ancestry.
            // This driver models completed replay identities, without transaction execution.
            let _ = bank_forks.remove(block.slot);
        }
        let bank = bank_forks.insert(bank).clone_without_scheduler();
        drop(bank_forks);
        self.replayed.insert(block, bank.clone());
        self.event(VotorEvent::Block(CompletedBlock {
            slot: block.slot,
            bank,
        }));
        true
    }

    pub(super) fn dead(&mut self, block: Block) {
        self.replayed.remove(&block);
        let mut bank_forks = self.shared.bank_forks.write().unwrap();
        if bank_forks.get(block.slot).is_some_and(|bank| {
            bank.block_id() == Some(block.block_id.to_hash()) && block.slot > bank_forks.root()
        }) {
            let _ = bank_forks.remove(block.slot);
        }
    }

    pub(super) fn has_replayed(&self, block: Block) -> bool {
        self.replayed.contains_key(&block)
    }

    pub(super) fn highest_finalized(&self) -> Slot {
        self.pool
            .highest_finalized_slot()
            .map(|slot| slot.slot())
            .unwrap_or(0)
    }

    pub(super) fn vote_root(&self) -> Slot {
        self.voting.vote_history.root()
    }

    pub(super) fn rooted_block(&self) -> Option<Block> {
        *self.requested_root.lock().unwrap()
    }

    pub(super) fn certificates(&self) -> &BTreeSet<CertificateType> {
        &self.certificates
    }

    pub(super) fn take_votes(&mut self) -> Vec<(Vote, bool)> {
        std::mem::take(&mut self.votes)
    }

    fn ingest(&mut self, message: PoolMessage, events: &mut VecDeque<VotorEvent>) {
        let mut generated_events = vec![];
        let bank = self.bank();
        let (_, certificates) = self
            .pool
            .add_pool_msg(&bank, message, &mut generated_events);
        self.certificates
            .extend(certificates.iter().map(|cert| cert.cert_type));
        events.extend(generated_events);
    }

    fn record_ops(&mut self, operations: impl IntoIterator<Item = BLSOp>) {
        for operation in operations {
            match operation {
                BLSOp::PushVote { vote } => self.votes.push((vote.vote, false)),
                BLSOp::RefreshVotes { votes } => {
                    self.votes
                        .extend(votes.iter().map(|vote| (vote.vote, true)));
                }
                BLSOp::PushCertificates { .. } | BLSOp::RefreshCertificates { .. } => (),
            }
        }
    }

    fn pump(&mut self, mut events: VecDeque<VotorEvent>) {
        loop {
            while let Some(event) = events.pop_front() {
                if event.should_ignore(self.bank().slot().max(self.vote_root())) {
                    continue;
                }
                let operations = EventHandler::handle_event(
                    event,
                    &self.timers,
                    &self.shared,
                    &mut self.voting,
                    &self.rooting,
                    &mut self.local,
                )
                .unwrap();
                self.record_ops(operations);
            }
            let operations = self.bls_ops.try_iter().collect::<Vec<_>>();
            self.record_ops(operations);
            let own_votes = self
                .own_votes
                .try_iter()
                .map(PoolVote::Own)
                .collect::<Vec<_>>();
            if !own_votes.is_empty() {
                self.ingest(PoolMessage::Votes(own_votes), &mut events);
            }
            self.pending_safe_to_notar
                .extend(self.pool.take_pending_safe_to_notar());
            let highest_finalized = self.highest_finalized();
            // Replay supplies the same parent relation that the service reads from blockstore.
            self.pending_safe_to_notar.retain(|block| {
                if block.slot <= highest_finalized {
                    return false;
                }
                let Some(bank) = self.replayed.get(block) else {
                    return true;
                };
                let (_, parent) = EventHandler::get_block_parent_block(bank);
                if self.pool.block_has_notar_fallback_or_stronger(parent) {
                    events.push_back(VotorEvent::SafeToNotar(*block));
                    false
                } else {
                    true
                }
            });
            (self.drain_auxiliary)();
            if events.is_empty() {
                break;
            }
        }
    }
}
