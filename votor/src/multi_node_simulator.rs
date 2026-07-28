//! Deterministic, single-threaded, in-process multi-node Votor simulator.
//!
//! This is the Alpenglow analog of `core/src/vote_simulator.rs` (which is TowerBFT-only).
//! It drives N nodes' real votor decision logic ([`EventHandler::handle_event`]) and real
//! consensus-pool aggregation ([`ConsensusPool::add_pool_msg`]) directly as pure synchronous
//! functions, routing the resulting votes/certs between nodes over an in-memory message bus.
//! No threads, no sockets, no wall clock.
//!
//! Each node holds its own `SharedContext` / `VotingContext` / `RootContext` / `LocalContext`
//! (built exactly like the single-node test in `event_handler.rs::setup`), its own
//! [`ConsensusPool`], and its own `BankForks`. The engine:
//!   1. manufactures a leader's block deterministically and feeds `VotorEvent::Block`,
//!   2. collects each node's `Vec<BLSOp>` output and its own-vote loopback,
//!   3. routes votes/certs into peers' pools, feeding the resulting `VotorEvent`s back until a
//!      fixpoint, and
//!   4. advances a virtual clock to fire timeouts (skip / crashed-leader paths).
//!
//! Repair is modeled at votor's abstraction: the engine can withhold a block from a node; when
//! that node emits `RepairEvent::FetchBlock`, the engine "repairs" by delivering the withheld
//! block. The real repair subsystem (serve_repair / shreds / replay) is out of scope.
#![cfg(feature = "dev-context-only-utils")]

use {
    crate::{
        consensus_pool::{ConsensusPool, parent_ready_tracker::BlockProductionParent},
        consensus_pool_service::{PoolMessage, PoolVote},
        event::{CompletedBlock, RepairEvent, VotorEvent},
        event_handler::{EventHandler, LocalContext},
        root_utils::{self, RootContext},
        timer_manager::TimerManager,
        vote_history::VoteHistory,
        vote_history_storage::NullVoteHistoryStorage,
        voting_service::BLSOp,
        voting_utils::VotingContext,
        votor::SharedContext,
    },
    agave_bls_sigverify::generated_cert_types::GeneratedCertTypes,
    agave_votor_messages::{
        certificate::Certificate,
        consensus_message::{Block, VoteMessage},
        finalized_slot::FinalizedSlot,
        migration::MigrationStatus,
        own_message::OwnMessage,
        sig_verified_messages::VoteAggregate,
    },
    crossbeam_channel::{Receiver, bounded},
    parking_lot::RwLock as PlRwLock,
    solana_clock::Slot,
    solana_gossip::{cluster_info::ClusterInfo, contact_info::ContactInfo},
    solana_hash::Hash,
    solana_ledger::{
        blockstore::Blockstore, blockstore_options::BlockstoreOptions,
        leader_schedule_cache::LeaderScheduleCache,
    },
    solana_net_utils::SocketAddrSpace,
    solana_pubkey::Pubkey,
    solana_runtime::{
        bank::{Bank, SlotLeader},
        bank_forks::BankForks,
        bank_forks_controller::{BankForksController, BankForksControllerError},
        genesis_utils::{
            GenesisConfigInfo, ValidatorVoteKeypairs,
            create_genesis_config_with_alpenglow_vote_accounts,
        },
        installed_scheduler_pool::BankWithScheduler,
        leader_schedule_utils::last_of_consecutive_leader_slots,
    },
    solana_signer::Signer,
    std::{
        collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
        sync::{
            Arc, RwLock,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    },
};

/// Amount of virtual time each [`VotorSimulator::tick`] advances. Large enough to fire an entire
/// leader window's timeouts in one step (first fire ~= `DELTA_TIMEOUT` + `delta_first_fec_set`).
const TICK: Duration = Duration::from_secs(10);

/// Unique-path counter for per-node blockstores.
static LEDGER_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Deterministic block id for `(slot, version)`. Honest slots use version 0; an equivocating
/// leader produces additional versions (distinct block ids) for the same slot.
fn block_id_for(slot: Slot, version: usize) -> Hash {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&slot.to_le_bytes());
    // Tag byte so slot 0 / version 0 does not collide with `Hash::default()` (genesis parent id).
    bytes[8] = 1;
    bytes[9] = version as u8;
    Hash::new_from_array(bytes)
}

/// Everything needed to (re)manufacture the canonical block for a slot, so a node that missed it
/// (partition / withhold) can be given the same block on repair.
#[derive(Clone, Copy)]
struct CanonicalBlock {
    block_id: Hash,
    parent_slot: Slot,
    parent_block_id: Hash,
    leader_id: Pubkey,
    leader_vote: Pubkey,
}

/// In-memory network policy. The seam for partitions, block withholding (skip/repair scenarios),
/// and — later — Byzantine behaviors. Honest-path default delivers everything.
#[derive(Default)]
pub struct NetworkPolicy {
    /// node index -> partition id; messages/blocks only flow within a partition. `None` = one net.
    partitions: Option<Vec<usize>>,
    /// (node, slot) pairs whose block is withheld from that node (delivered later on repair).
    withheld: HashSet<(usize, Slot)>,
    /// slots whose block is withheld from every node (crashed / silent leader).
    crashed_slots: HashSet<Slot>,
    /// Equivocated slots: `block_version[slot][node]` is the block version that `node` receives
    /// (`usize::MAX` = no block). Absent slot => every node receives version 0 (honest).
    block_version: HashMap<Slot, Vec<usize>>,
    /// Equivocated slots: number of distinct block versions the leader produces. Absent => 1.
    num_versions: HashMap<Slot, usize>,
}

impl NetworkPolicy {
    pub fn deliver_all() -> Self {
        Self::default()
    }

    /// Assign nodes to partitions (each inner slice is a partition of node indices).
    pub fn with_partitions(mut self, groups: &[&[usize]], num_nodes: usize) -> Self {
        let mut map = vec![usize::MAX; num_nodes];
        for (pid, group) in groups.iter().enumerate() {
            for &n in *group {
                map[n] = pid;
            }
        }
        self.partitions = Some(map);
        self
    }

    /// Withhold the block for `slot` from `node` until repair delivers it.
    pub fn withhold_block(mut self, node: usize, slot: Slot) -> Self {
        self.withheld.insert((node, slot));
        self
    }

    /// Withhold the block for `slot` from everyone (simulate a silent leader -> skip votes).
    pub fn crash_slot(mut self, slot: Slot) -> Self {
        self.crashed_slots.insert(slot);
        self
    }

    /// Make the leader of `slot` equivocate: it produces one distinct block per entry in `groups`,
    /// and the nodes in `groups[v]` each receive version `v`. Nodes in no group receive no block.
    ///
    /// e.g. `equivocate(2, &[&[0, 1], &[2, 3]], 4)` sends block A to nodes 0,1 and block B to 2,3.
    pub fn equivocate(mut self, slot: Slot, groups: &[&[usize]], num_nodes: usize) -> Self {
        let mut versions = vec![usize::MAX; num_nodes];
        for (v, group) in groups.iter().enumerate() {
            for &n in *group {
                versions[n] = v;
            }
        }
        self.num_versions.insert(slot, groups.len());
        self.block_version.insert(slot, versions);
        self
    }

    fn same_partition(&self, a: usize, b: usize) -> bool {
        match &self.partitions {
            None => true,
            Some(map) => map[a] == map[b],
        }
    }

    fn allow_message(&self, from: usize, to: usize) -> bool {
        self.same_partition(from, to)
    }

    /// Number of distinct block versions the leader produces for `slot` (1 unless equivocating).
    fn num_versions_for(&self, slot: Slot) -> usize {
        self.num_versions.get(&slot).copied().unwrap_or(1)
    }

    /// Which block version `to` should receive for `slot`, or `None` if it receives no block.
    fn block_delivery(&self, leader: usize, to: usize, slot: Slot) -> Option<usize> {
        if self.crashed_slots.contains(&slot)
            || self.withheld.contains(&(to, slot))
            || !self.same_partition(leader, to)
        {
            return None;
        }
        match self.block_version.get(&slot) {
            None => Some(0),
            Some(versions) => match versions[to] {
                usize::MAX => None,
                v => Some(v),
            },
        }
    }
}

/// In-process [`BankForksController`] backed directly by an `Arc<RwLock<BankForks>>`, so votor's
/// root path runs synchronously with no replay. Mirrors `event_handler::tests::DirectBankForksController`.
struct SimBankForksController {
    my_pubkey: Pubkey,
    bank_forks: Arc<RwLock<BankForks>>,
    blockstore: Arc<Blockstore>,
    leader_schedule_cache: Arc<LeaderScheduleCache>,
    drop_bank_sender: crossbeam_channel::Sender<Vec<BankWithScheduler>>,
}

impl BankForksController for SimBankForksController {
    fn insert_bank(&self, bank: Bank) -> Result<BankWithScheduler, BankForksControllerError> {
        Ok(self.bank_forks.write().unwrap().insert(bank))
    }

    fn enqueue_set_root(
        &self,
        parent_slot: Slot,
        new_root: Slot,
        highest_super_majority_root: Option<Slot>,
    ) {
        root_utils::check_and_handle_new_root(
            parent_slot,
            new_root,
            None,
            highest_super_majority_root,
            &None,
            &self.drop_bank_sender,
            &self.blockstore,
            &self.leader_schedule_cache,
            &self.bank_forks,
            None,
            &self.my_pubkey,
            |_| {},
        );
    }

    fn clear_bank(&self, slot: Slot) -> Result<(), BankForksControllerError> {
        let bank_to_clear = self.bank_forks.read().unwrap().get_with_scheduler(slot);
        if let Some(bank) = bank_to_clear {
            let _ = bank.wait_for_completed_scheduler();
        }
        self.bank_forks.write().unwrap().clear_bank(slot, false);
        Ok(())
    }
}

/// One simulated node: the real votor contexts + pool + captured output channels.
struct SimNode {
    node_pubkey: Pubkey,
    vote_pubkey: Pubkey,
    bank_forks: Arc<RwLock<BankForks>>,
    pool: ConsensusPool,
    timer_manager: Arc<PlRwLock<TimerManager>>,
    shared_context: SharedContext,
    voting_context: VotingContext,
    root_context: RootContext,
    local_context: LocalContext,

    // Captured outputs (sender halves live inside the contexts / timer).
    own_vote_receiver: Receiver<OwnMessage>,
    repair_event_receiver: Receiver<RepairEvent>,
    timer_event_receiver: Receiver<VotorEvent>,

    // Kept alive so the corresponding senders inside the node never see a dropped receiver.
    _bls_receiver: Receiver<BLSOp>,
    _commitment_receiver: Receiver<crate::commitment::CommitmentAggregationData>,
    _reward_receiver: Receiver<agave_bls_sigverify::rewards::RewardInput>,
    _consensus_metrics_receiver: agave_votor_messages::metric_types::ConsensusMetricsEventReceiver,
    _leader_window_info_receiver: Receiver<crate::event::LeaderWindowInfo>,
    _drop_bank_receiver: Receiver<Vec<BankWithScheduler>>,
    _generated_cert_types: Arc<GeneratedCertTypes>,
}

/// A unit of work in the in-memory routing fixpoint.
enum Work {
    /// Feed `event` to `node`'s `handle_event`.
    Event { node: usize, event: VotorEvent },
    /// Add a vote to `node`'s pool (own loopback or external broadcast).
    Vote {
        node: usize,
        msg: VoteMessage,
        own: bool,
    },
    /// Add a certificate to `node`'s pool.
    Cert {
        node: usize,
        cert: Arc<Certificate>,
    },
}

/// Deterministic in-process multi-node votor engine.
pub struct VotorSimulator {
    nodes: Vec<SimNode>,
    validators: Vec<ValidatorVoteKeypairs>,
    genesis: GenesisConfigInfo,
    leader_schedule_cache: Arc<LeaderScheduleCache>,
    net: NetworkPolicy,
    virtual_now: Arc<PlRwLock<Instant>>,

    /// Window start-slots already produced, to avoid double production.
    produced_windows: HashSet<Slot>,
    /// Every block the sim has manufactured, keyed by block id, for repair re-delivery. A node
    /// requests a specific block id via `FetchBlock`, so this lets the sim serve exactly that block
    /// (including the majority block A to a node stuck on an equivocation fork B).
    canonical_blocks: HashMap<Hash, CanonicalBlock>,
    /// Repair requests (`FetchBlock`) collected during the fixpoint.
    pending_repair: Vec<(usize, Block)>,
    /// Which block id each node finalized per slot (from `VotorEvent::Finalized`), for safety checks.
    finalized_block_ids: HashMap<(usize, Slot), Hash>,
    /// Do not produce windows whose start exceeds this (bounds the honest cascade).
    max_produce_slot: Slot,
    /// Safety bound on time-advancing ticks.
    max_ticks: usize,
}

impl VotorSimulator {
    /// Build an N-node cluster with the given per-node stakes, all sharing one Alpenglow genesis.
    pub fn new(stakes: Vec<u64>) -> Self {
        let validators = (0..stakes.len())
            .map(|_| ValidatorVoteKeypairs::new_rand())
            .collect::<Vec<_>>();
        let genesis = create_genesis_config_with_alpenglow_vote_accounts(
            1_000_000_000,
            &validators,
            stakes,
        );
        let root_bank_for_schedule = Bank::new_for_tests(&genesis.genesis_config);
        let leader_schedule_cache =
            Arc::new(LeaderScheduleCache::new_from_bank(&root_bank_for_schedule));
        let virtual_now = Arc::new(PlRwLock::new(Instant::now()));

        let mut sim = Self {
            nodes: Vec::new(),
            validators,
            genesis,
            leader_schedule_cache,
            net: NetworkPolicy::deliver_all(),
            virtual_now,
            produced_windows: HashSet::new(),
            canonical_blocks: HashMap::new(),
            pending_repair: Vec::new(),
            finalized_block_ids: HashMap::new(),
            max_produce_slot: 0,
            max_ticks: 200,
        };
        for index in 0..sim.validators.len() {
            let node = sim.build_node(index);
            sim.nodes.push(node);
        }
        sim
    }

    pub fn with_network(mut self, net: NetworkPolicy) -> Self {
        self.net = net;
        self
    }

    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Bound on time-advancing ticks in [`VotorSimulator::run_until_finalized`].
    pub fn set_max_ticks(&mut self, max_ticks: usize) {
        self.max_ticks = max_ticks;
    }

    fn build_node(&self, index: usize) -> SimNode {
        let keys = &self.validators[index];
        let node_keypair = keys.node_keypair.insecure_clone();
        let vote_keypair = keys.vote_keypair.insecure_clone();
        let node_pubkey = node_keypair.pubkey();
        let vote_pubkey = vote_keypair.pubkey();

        let (bls_sender, bls_receiver) = bounded(4096);
        let (commitment_sender, commitment_receiver) = bounded(4096);
        let (own_vote_sender, own_vote_receiver) = bounded(4096);
        let (reward_sender, reward_receiver) = bounded(4096);
        let (drop_bank_sender, drop_bank_receiver) = bounded(4096);
        let (consensus_metrics_sender, consensus_metrics_receiver) = bounded(4096);
        let (leader_window_info_sender, leader_window_info_receiver) = bounded(4096);
        let (repair_event_sender, repair_event_receiver) = bounded(4096);
        // The timer's event sender feeds this node's event channel; we drain it after `progress`.
        let (timer_event_sender, timer_event_receiver) = bounded(4096);

        let bank0 = Bank::new_for_tests(&self.genesis.genesis_config);
        let bank_forks = BankForks::new_rw_arc(bank0);
        let root_bank = bank_forks.read().unwrap().root_bank();

        let contact_info = ContactInfo::new_localhost(&node_pubkey, 0);
        let cluster_info = Arc::new(ClusterInfo::new(
            contact_info,
            Arc::new(node_keypair.insecure_clone()),
            SocketAddrSpace::Unspecified,
        ));

        let ledger_id = LEDGER_COUNTER.fetch_add(1, Ordering::Relaxed);
        let ledger_path = std::env::temp_dir().join(format!(
            "votor-sim-{}-{}",
            std::process::id(),
            ledger_id
        ));
        let blockstore = Arc::new(
            Blockstore::open_with_options(&ledger_path, BlockstoreOptions::default_for_tests())
                .unwrap(),
        );

        let leader_schedule_cache = Arc::new(LeaderScheduleCache::new_from_bank(&root_bank));

        let bank_forks_controller = Arc::new(SimBankForksController {
            my_pubkey: node_pubkey,
            bank_forks: bank_forks.clone(),
            blockstore: blockstore.clone(),
            leader_schedule_cache: leader_schedule_cache.clone(),
            drop_bank_sender,
        });

        let highest_parent_ready = Arc::new(RwLock::default());
        let vote_history_storage = Arc::new(NullVoteHistoryStorage::default());
        let latest_switch_request = crate::event::LatestSwitchRequest::default();

        let timer_manager = Arc::new(PlRwLock::new(TimerManager::new_manual(
            timer_event_sender,
            self.virtual_now.clone(),
        )));

        let shared_context = SharedContext {
            cluster_info: cluster_info.clone(),
            bank_forks: bank_forks.clone(),
            vote_history_storage: vote_history_storage.clone(),
            leader_window_info_sender,
            blockstore: blockstore.clone(),
            highest_parent_ready,
            repair_event_sender,
            latest_switch_request,
        };

        let vote_history = VoteHistory::new(node_pubkey, 0);
        let voting_context = VotingContext {
            cluster_info: cluster_info.clone(),
            leader_schedule: leader_schedule_cache.clone(),
            vote_history,
            vote_account_pubkey: vote_pubkey,
            identity_keypair: Arc::new(node_keypair.insecure_clone()),
            authorized_voter_keypairs: Arc::new(RwLock::new(vec![Arc::new(vote_keypair)])),
            vote_history_storage,
            derived_bls_keypairs: HashMap::new(),
            own_vote_sender,
            own_reward_sender: reward_sender,
            bls_sender,
            commitment_sender,
            wait_to_vote_slot: None,
            sharable_banks: bank_forks.read().unwrap().sharable_banks(),
            consensus_metrics_sender,
        };

        let root_context = RootContext {
            bank_notification_sender: None,
            bank_forks_controller,
        };

        let local_context = LocalContext {
            my_pubkey: node_pubkey,
            pending_blocks: BTreeMap::new(),
            finalized_blocks: BTreeSet::new(),
            received_shred: BTreeSet::new(),
            stats: Default::default(),
            standstill_slot: None,
        };

        // Consensus pool, seeded exactly like `ConsensusPoolService::main_loop`.
        let generated_cert_types = Arc::new(GeneratedCertTypes::default());
        let root_block = Block {
            slot: root_bank.slot(),
            block_id: root_bank.block_id().unwrap_or_default(),
        };
        let initial_parent_ready = (root_bank.slot().checked_add(1).unwrap(), root_block);
        let pool = ConsensusPool::new(
            cluster_info,
            &root_bank,
            generated_cert_types.clone(),
            Arc::new(MigrationStatus::post_migration_status()),
            initial_parent_ready,
        );

        SimNode {
            node_pubkey,
            vote_pubkey,
            bank_forks,
            pool,
            timer_manager,
            shared_context,
            voting_context,
            root_context,
            local_context,
            own_vote_receiver,
            repair_event_receiver,
            timer_event_receiver,
            _bls_receiver: bls_receiver,
            _commitment_receiver: commitment_receiver,
            _reward_receiver: reward_receiver,
            _consensus_metrics_receiver: consensus_metrics_receiver,
            _leader_window_info_receiver: leader_window_info_receiver,
            _drop_bank_receiver: drop_bank_receiver,
            _generated_cert_types: generated_cert_types,
        }
    }

    /// The genesis root block shared by all nodes.
    fn root_block(&self) -> Block {
        let root_bank = self.nodes[0].bank_forks.read().unwrap().root_bank();
        Block {
            slot: root_bank.slot(),
            block_id: root_bank.block_id().unwrap_or_default(),
        }
    }

    /// Feed one `VotorEvent` to a node's `handle_event`, returning the votes/certs it wants to send.
    fn handle(&mut self, node: usize, event: VotorEvent) -> Vec<BLSOp> {
        let n = &mut self.nodes[node];
        EventHandler::handle_event(
            event,
            &n.timer_manager,
            &n.shared_context,
            &mut n.voting_context,
            &n.root_context,
            &mut n.local_context,
        )
        .unwrap()
    }

    /// Add a `PoolMessage` to a node's pool, returning (new finalized slot, generated certs, events).
    fn pool_add(
        &mut self,
        node: usize,
        msg: PoolMessage,
    ) -> (Option<Slot>, Vec<Arc<Certificate>>, Vec<VotorEvent>) {
        let n = &mut self.nodes[node];
        let root_bank = n.bank_forks.read().unwrap().root_bank();
        let mut events = vec![];
        let (fin, certs) = n.pool.add_pool_msg(&root_bank, msg, &mut events);
        (fin, certs, events)
    }

    fn to_aggregate(&self, node: usize, msg: VoteMessage) -> VoteAggregate {
        let root_bank = self.nodes[node].bank_forks.read().unwrap().root_bank();
        let rank_map = root_bank
            .epoch_stakes_from_slot(msg.vote.slot())
            .unwrap()
            .bls_pubkey_to_rank_map();
        VoteAggregate::new_from_verified_vote(rank_map.len(), msg)
    }

    /// Nodes reachable from `from` for consensus messages, per the network policy.
    fn message_targets(&self, from: usize) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|&to| to != from && self.net.allow_message(from, to))
            .collect()
    }

    /// Route a node's `BLSOp` outputs onto the queue (votes/certs to peers).
    fn route_ops(&mut self, from: usize, ops: Vec<BLSOp>, queue: &mut VecDeque<Work>) {
        let targets = self.message_targets(from);
        for op in ops {
            match op {
                BLSOp::PushVote { vote } => {
                    for &to in &targets {
                        queue.push_back(Work::Vote {
                            node: to,
                            msg: (*vote).clone(),
                            own: false,
                        });
                    }
                }
                BLSOp::RefreshVotes { votes } => {
                    for vote in votes {
                        for &to in &targets {
                            queue.push_back(Work::Vote {
                                node: to,
                                msg: (*vote).clone(),
                                own: false,
                            });
                        }
                    }
                }
                BLSOp::PushCertificates { certificates }
                | BLSOp::RefreshCertificates { certificates } => {
                    for cert in certificates {
                        for &to in &targets {
                            queue.push_back(Work::Cert {
                                node: to,
                                cert: cert.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    /// Broadcast pool-generated certs to peers and feed pool events back to the same node.
    fn absorb_pool_output(
        &mut self,
        node: usize,
        certs: Vec<Arc<Certificate>>,
        events: Vec<VotorEvent>,
        queue: &mut VecDeque<Work>,
    ) {
        let targets = self.message_targets(node);
        for cert in certs {
            for &to in &targets {
                queue.push_back(Work::Cert {
                    node: to,
                    cert: cert.clone(),
                });
            }
        }
        for event in events {
            if let VotorEvent::ParentReady { slot, .. } = &event {
                self.maybe_produce(node, *slot, queue);
            }
            queue.push_back(Work::Event { node, event });
        }
    }

    /// Replicates `ConsensusPoolService::add_produce_block_event`: if `node` is the leader for the
    /// parent-ready `slot`, manufacture and disseminate its leader window.
    fn maybe_produce(&mut self, node: usize, slot: Slot, queue: &mut VecDeque<Work>) {
        if slot > self.max_produce_slot {
            return;
        }
        let root_bank = self.nodes[node].bank_forks.read().unwrap().root_bank();
        let leader = self
            .leader_schedule_cache
            .slot_leader_at(slot, Some(&root_bank))
            .map(|l| l.id);
        if leader != Some(self.nodes[node].node_pubkey) {
            return;
        }
        if !self.produced_windows.insert(slot) {
            return;
        }
        let parent_block = match self.nodes[node]
            .pool
            .parent_ready_tracker
            .block_production_parent(slot)
        {
            BlockProductionParent::Parent(p) => p,
            _ => return,
        };
        let end = last_of_consecutive_leader_slots(slot);
        self.produce_window(node, slot, end, parent_block, queue);
    }

    fn produce_window(
        &mut self,
        leader_node: usize,
        start: Slot,
        end: Slot,
        parent_block: Block,
        queue: &mut VecDeque<Work>,
    ) {
        let leader_id = self.nodes[leader_node].node_pubkey;
        let leader_vote = self.nodes[leader_node].vote_pubkey;
        let mut parent = parent_block;
        for slot in start..=end {
            let versions = self.net.num_versions_for(slot);
            // Manufacture every version of this slot's block, all chained from the same parent.
            let cbs: Vec<CanonicalBlock> = (0..versions)
                .map(|v| {
                    let cb = CanonicalBlock {
                        block_id: block_id_for(slot, v),
                        parent_slot: parent.slot,
                        parent_block_id: parent.block_id,
                        leader_id,
                        leader_vote,
                    };
                    self.canonical_blocks.insert(cb.block_id, cb);
                    cb
                })
                .collect();
            // Deliver each node the version the network policy assigns it.
            for to in 0..self.nodes.len() {
                if let Some(v) = self.net.block_delivery(leader_node, to, slot) {
                    self.deliver_block(to, slot, cbs[v], queue);
                }
            }
            if versions > 1 {
                // An equivocating leader cannot cleanly continue its window; the honest cluster
                // recovers via timeout -> skip and the next leader's window (chained via a fresh
                // parent-ready). Stop producing the rest of this window.
                break;
            }
            parent = Block {
                slot,
                block_id: block_id_for(slot, 0),
            };
        }
    }

    /// Build the canonical block for `slot` in `node`'s bank_forks and feed `VotorEvent::Block`.
    fn deliver_block(
        &self,
        node: usize,
        slot: Slot,
        cb: CanonicalBlock,
        queue: &mut VecDeque<Work>,
    ) {
        let n = &self.nodes[node];
        if n.bank_forks.read().unwrap().get(slot).is_some() {
            return; // already have it
        }
        let Some(parent_bank) = n.bank_forks.read().unwrap().get(cb.parent_slot) else {
            return; // missing parent; a later repair round will handle it
        };
        // If this node's parent is a different block (it followed the other side of an
        // equivocation), it cannot chain this child — leave it on its own fork.
        if cb.parent_slot != 0 && parent_bank.block_id() != Some(cb.parent_block_id) {
            return;
        }
        let leader = SlotLeader {
            id: cb.leader_id,
            vote_address: cb.leader_vote,
        };
        let bank = Bank::new_from_parent(parent_bank, leader, slot);
        bank.set_block_id(Some(cb.block_id));
        bank.freeze();
        n.bank_forks.write().unwrap().insert(bank);
        let bank = n.bank_forks.read().unwrap().get(slot).unwrap();
        queue.push_back(Work::Event {
            node,
            event: VotorEvent::Block(CompletedBlock { slot, bank }),
        });
    }

    /// Drive the queue to a fixpoint, routing all votes/certs/events between nodes.
    fn run_fixpoint(&mut self, queue: &mut VecDeque<Work>) {
        while let Some(work) = queue.pop_front() {
            match work {
                Work::Event { node, event } => {
                    // Drop stale events exactly as the real event loop does (event_handler.rs),
                    // so a lagging/forked node never votes below its root.
                    let ignore_root = {
                        let vctx = &self.nodes[node].voting_context;
                        vctx.sharable_banks
                            .root()
                            .slot()
                            .max(vctx.vote_history.root())
                    };
                    if event.should_ignore(ignore_root) {
                        continue;
                    }
                    let produce_slot = match &event {
                        VotorEvent::ParentReady { slot, .. } => Some(*slot),
                        _ => None,
                    };
                    // Record what each node finalizes (block id per slot) for the safety check.
                    if let VotorEvent::Finalized(block, _) = &event {
                        self.finalized_block_ids
                            .insert((node, block.slot), block.block_id);
                    }
                    let ops = self.handle(node, event);

                    // Own-vote loopback into this node's own pool.
                    while let Ok(msg) = self.nodes[node].own_vote_receiver.try_recv() {
                        if let OwnMessage::Vote(v) = msg {
                            queue.push_back(Work::Vote {
                                node,
                                msg: v,
                                own: true,
                            });
                        }
                    }
                    // Collect repair requests for later modeled repair.
                    while let Ok(RepairEvent::FetchBlock { block }) =
                        self.nodes[node].repair_event_receiver.try_recv()
                    {
                        self.pending_repair.push((node, block));
                    }

                    if let Some(slot) = produce_slot {
                        self.maybe_produce(node, slot, queue);
                    }
                    self.route_ops(node, ops, queue);
                }
                Work::Vote { node, msg, own } => {
                    let pool_vote = if own {
                        PoolVote::Own(msg)
                    } else {
                        PoolVote::External(self.to_aggregate(node, msg))
                    };
                    let (_fin, certs, events) =
                        self.pool_add(node, PoolMessage::Votes(vec![pool_vote]));
                    self.absorb_pool_output(node, certs, events, queue);
                }
                Work::Cert { node, cert } => {
                    let (_fin, certs, events) =
                        self.pool_add(node, PoolMessage::Certificates(vec![(*cert).clone()]));
                    self.absorb_pool_output(node, certs, events, queue);
                }
            }
        }
    }

    /// Advance the virtual clock by [`TICK`] and fire any due timeouts into the fixpoint.
    fn tick(&mut self) {
        let now = *self.virtual_now.read() + TICK;
        *self.virtual_now.write() = now;
        let mut queue = VecDeque::new();
        for i in 0..self.nodes.len() {
            self.nodes[i].timer_manager.read().progress(now);
            while let Ok(event) = self.nodes[i].timer_event_receiver.try_recv() {
                queue.push_back(Work::Event { node: i, event });
            }
        }
        self.run_fixpoint(&mut queue);
    }

    /// Serve the exact blocks nodes have requested via `FetchBlock` (`pending_repair`).
    ///
    /// This handles both plain catch-up (a node that simply missed a block) and equivocation
    /// re-convergence: a node stuck on the losing fork requests the finalized block id, and here we
    /// drop its divergent block for that slot and deliver the requested one so votor can root to it.
    /// Returns whether anything was delivered.
    fn process_repairs(&mut self, queue: &mut VecDeque<Work>) -> bool {
        let requests = std::mem::take(&mut self.pending_repair);
        let mut seen = HashSet::new();
        let mut delivered = false;
        for (node, block) in requests {
            if !seen.insert((node, block)) {
                continue;
            }
            // Only serve blocks the sim actually produced.
            let Some(cb) = self.canonical_blocks.get(&block.block_id).copied() else {
                continue;
            };
            // Already hold exactly the requested block? Nothing to do.
            let have = self.nodes[node].bank_forks.read().unwrap().get(block.slot);
            if have.as_ref().and_then(|b| b.block_id()) == Some(block.block_id) {
                continue;
            }
            // The parent must be present and be the matching block, else repair it first (retry).
            let parent_ok = cb.parent_slot == 0
                || self.nodes[node]
                    .bank_forks
                    .read()
                    .unwrap()
                    .get(cb.parent_slot)
                    .and_then(|p| p.block_id())
                    == Some(cb.parent_block_id);
            if !parent_ok {
                self.pending_repair.push((node, block));
                continue;
            }
            // If a divergent block occupies the slot (losing equivocation fork), drop it first.
            if have.is_some() {
                self.clear_bank_for_switch(node, block.slot);
            }
            self.deliver_block(node, block.slot, cb, queue);
            delivered = true;
        }
        delivered
    }

    /// Drop a node's (non-rooted) block at `slot` so a different block can be delivered there —
    /// the sim's stand-in for votor's duplicate-block dump-and-repair. Mirrors
    /// `SimBankForksController::clear_bank`.
    fn clear_bank_for_switch(&self, node: usize, slot: Slot) {
        let bank_forks = &self.nodes[node].bank_forks;
        if slot <= bank_forks.read().unwrap().root_bank().slot() {
            return; // never disturb the rooted chain
        }
        let to_clear = bank_forks.read().unwrap().get_with_scheduler(slot);
        if let Some(bank) = to_clear {
            let _ = bank.wait_for_completed_scheduler();
        }
        bank_forks.write().unwrap().clear_bank(slot, false);
    }

    /// Run the cluster until every node has finalized at least `target`, or the tick budget runs out.
    pub fn run_until_finalized(&mut self, target: Slot) {
        self.max_produce_slot = last_of_consecutive_leader_slots(target);

        let root_block = self.root_block();
        let mut queue = VecDeque::new();
        for i in 0..self.nodes.len() {
            queue.push_back(Work::Event {
                node: i,
                event: VotorEvent::ParentReady {
                    slot: 1,
                    parent_block: root_block,
                },
            });
        }
        self.run_fixpoint(&mut queue);

        let mut ticks = 0;
        while self.min_finalized() < target && ticks < self.max_ticks {
            let mut queue = VecDeque::new();
            if self.process_repairs(&mut queue) {
                self.run_fixpoint(&mut queue);
            } else {
                self.tick();
            }
            ticks += 1;
        }
    }

    /// Highest finalized slot known to `node`.
    pub fn finalized_slot(&self, node: usize) -> Option<Slot> {
        self.nodes[node]
            .pool
            .highest_finalized_slot()
            .map(|s: FinalizedSlot| s.slot())
    }

    /// Minimum finalized slot across all nodes (0 if any node has finalized nothing).
    pub fn min_finalized(&self) -> Slot {
        self.nodes
            .iter()
            .map(|n| {
                n.pool
                    .highest_finalized_slot()
                    .map(|s| s.slot())
                    .unwrap_or(0)
            })
            .min()
            .unwrap_or(0)
    }

    /// Maximum finalized slot across all nodes.
    pub fn max_finalized(&self) -> Slot {
        self.nodes
            .iter()
            .map(|n| {
                n.pool
                    .highest_finalized_slot()
                    .map(|s| s.slot())
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0)
    }

    /// The block id `node` finalized at `slot`, if any (observed from `VotorEvent::Finalized`).
    pub fn finalized_block_id(&self, node: usize, slot: Slot) -> Option<Hash> {
        self.finalized_block_ids.get(&(node, slot)).copied()
    }

    /// Core safety property: no two nodes finalize different blocks for the same slot.
    /// A `false` here under equivocation is a genuine consensus safety violation.
    pub fn no_conflicting_finalizations(&self) -> bool {
        let mut by_slot: HashMap<Slot, Hash> = HashMap::new();
        for ((_, slot), block_id) in &self.finalized_block_ids {
            match by_slot.get(slot) {
                Some(existing) if existing != block_id => return false,
                _ => {
                    by_slot.insert(*slot, *block_id);
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_all_nodes_finalize() {
        let mut sim = VotorSimulator::new(vec![100; 4]);
        sim.run_until_finalized(8);
        assert!(
            sim.min_finalized() >= 8,
            "all nodes should finalize slot 8, got min={}",
            sim.min_finalized()
        );
    }

    #[test]
    fn crashed_leader_slot_is_skipped() {
        // Withhold slot 1's block from everyone; the cluster should skip it and still finalize later.
        let net = NetworkPolicy::deliver_all().crash_slot(1);
        let mut sim = VotorSimulator::new(vec![100; 4]).with_network(net);
        sim.run_until_finalized(8);
        assert!(
            sim.min_finalized() >= 8,
            "cluster should skip the crashed slot and finalize slot 8, got min={}",
            sim.min_finalized()
        );
    }

    #[test]
    fn minority_node_repairs_withheld_block() {
        // Withhold slot 2's block from node 0 only; it should repair and catch up.
        let net = NetworkPolicy::deliver_all().withhold_block(0, 2);
        let mut sim = VotorSimulator::new(vec![100; 4]).with_network(net);
        sim.run_until_finalized(8);
        assert!(
            sim.finalized_slot(0).unwrap_or(0) >= 8,
            "node 0 should repair and finalize slot 8, got {:?}",
            sim.finalized_slot(0)
        );
    }

    #[test]
    fn equivocation_preserves_safety() {
        // The leader of slot 2 equivocates: block A to the {0,1,2} supermajority, block B to node 3.
        // Safety must hold (no slot finalizes two different blocks), and the honest majority must
        // still make progress past the equivocated slot.
        let net = NetworkPolicy::deliver_all().equivocate(2, &[&[0, 1, 2], &[3]], 4);
        let mut sim = VotorSimulator::new(vec![100; 4]).with_network(net);
        sim.run_until_finalized(8);

        assert!(
            sim.no_conflicting_finalizations(),
            "SAFETY VIOLATION: two different blocks finalized for the same slot under equivocation"
        );
        // The supermajority that saw block A recovers and keeps finalizing.
        assert!(
            sim.finalized_slot(0).unwrap_or(0) >= 8,
            "honest supermajority should progress past the equivocation, got {:?}",
            sim.finalized_slot(0)
        );
        // Whichever nodes finalized slot 2 finalized block A (version 0), never node 3's block B.
        let block_a = block_id_for(2, 0);
        let block_b = block_id_for(2, 1);
        for node in 0..sim.num_nodes() {
            if let Some(id) = sim.finalized_block_id(node, 2) {
                assert_eq!(id, block_a, "node {node} finalized a non-majority block for slot 2");
                assert_ne!(id, block_b);
            }
        }
    }

    #[test]
    fn equivocation_minority_reconverges() {
        // Same majority-split equivocation, but assert the losing minority (node 3, which received
        // block B) repairs the finalized block A and re-converges: every node finalizes slot 8.
        let net = NetworkPolicy::deliver_all().equivocate(2, &[&[0, 1, 2], &[3]], 4);
        let mut sim = VotorSimulator::new(vec![100; 4]).with_network(net);
        sim.run_until_finalized(8);

        assert!(sim.no_conflicting_finalizations(), "safety must hold");
        assert!(
            sim.min_finalized() >= 8,
            "the minority node should re-converge and every node finalize slot 8, got min={} (node3={:?})",
            sim.min_finalized(),
            sim.finalized_slot(3),
        );
        // Node 3 must have switched off block B onto the finalized block A for slot 2.
        assert_eq!(sim.finalized_block_id(3, 2), Some(block_id_for(2, 0)));
    }

    #[test]
    fn even_split_equivocation_finalizes_nothing_at_that_slot() {
        // A 2/2 equivocation: neither block reaches the 60% notarization threshold, so slot 2 must
        // never finalize either block. Safety holds trivially, but this asserts the liveness edge:
        // no conflicting block sneaks through.
        let net = NetworkPolicy::deliver_all().equivocate(2, &[&[0, 1], &[2, 3]], 4);
        let mut sim = VotorSimulator::new(vec![100; 4]).with_network(net);
        sim.set_max_ticks(20);
        sim.run_until_finalized(8);

        assert!(sim.no_conflicting_finalizations(), "safety must hold under an even split");
        for node in 0..sim.num_nodes() {
            assert_eq!(
                sim.finalized_block_id(node, 2),
                None,
                "no node may finalize a block for the evenly-equivocated slot 2"
            );
        }
    }

    #[test]
    fn even_partition_halts_finalization() {
        // Split 4 equal-stake nodes 2/2. Neither half has the 60% needed to notarize or
        // skip-certify, so the cluster must make no finalization progress.
        let net = NetworkPolicy::deliver_all().with_partitions(&[&[0, 1], &[2, 3]], 4);
        let mut sim = VotorSimulator::new(vec![100; 4]).with_network(net);
        sim.set_max_ticks(10);
        sim.run_until_finalized(4);
        assert_eq!(
            sim.min_finalized(),
            0,
            "a partition with no quorum must not finalize"
        );
    }
}
