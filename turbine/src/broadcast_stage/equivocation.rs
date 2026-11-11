//! Scheduled Alpenglow leader equivocation for development clusters.
//!
//! Each selected slot gets a second, independently executed block containing one
//! system transfer. Fund the transfer signer in the common parent and do not
//! submit that transfer to the normal banking stage. Alternate blocks are sent
//! directly to the listed TVU peers and retained in Blockstore's alternate
//! columns. Block-ID repair serves either version; slot-only repair serves the
//! original. A uses normal Turbine unless `original_recipients` overrides its
//! initial delivery. This does not create a lasting network partition. Only UDP
//! and Alpenglow parents are supported.
//!
//! Set `ValidatorConfig::equivocation_schedule` on the leader to a map from
//! absolute slots to `AlternateBlockPlan`s. Transactions are signed against the
//! actual parent at production time. Each selected slot is attempted once; a
//! failed transfer or unavailable recipient is reported as a broadcast error.

use {
    super::{
        broadcast_utils::{self, BroadcastItem},
        standard_broadcast_run::StandardBroadcastRun,
        *,
    },
    solana_entry::{
        block_component::{
            BlockComponent, BlockFooterV1, VersionedBlockFooter, VersionedBlockMarker,
        },
        entry::Entry,
    },
    solana_hash::Hash,
    solana_ledger::shred::{ProcessShredsStats, Shredder, merkle_tree::MerkleTree},
    solana_runtime::{
        bank::Bank, block_component_processor::BlockComponentProcessor,
        validated_block_finalization::ValidatedBlockFinalizationCert,
        validated_reward_certificate::ValidatedRewardCert,
    },
    solana_sha256_hasher::hashv,
    solana_signer::Signer,
    std::collections::BTreeMap,
};

/// Absolute slots, evaluated only on the scheduled leader. Empty means disabled.
pub type EquivocationSchedule = BTreeMap<Slot, AlternateBlockPlan>;

#[derive(Clone, Debug)]
pub struct AlternateBlockPlan {
    /// Must have enough funds for the transfer and fee in the common parent.
    pub payer: Arc<Keypair>,
    pub recipient: Pubkey,
    pub lamports: u64,
    /// Validator identities, resolved to their UDP TVU addresses through gossip.
    pub recipients: Vec<Pubkey>,
    /// Override A's initial delivery for this slot. None uses normal Turbine;
    /// Some(empty) retains A locally without broadcasting it. Recipients can
    /// still retransmit either version, so this does not create a partition.
    pub original_recipients: Option<Vec<Pubkey>>,
    /// Optional observation of the two produced blocks, after B is persisted.
    /// A full or disconnected observer channel does not block production.
    pub produced_sender: Option<Sender<EquivocationResult>>,
}

#[derive(Clone, Copy, Debug)]
pub struct EquivocationResult {
    pub slot: Slot,
    pub original_block_id: Hash,
    pub original_bank_hash: Hash,
    pub alternate_block_id: Hash,
    pub alternate_bank_hash: Hash,
}

struct AlternateBlock {
    shreds: Vec<Shred>,
    bank_hash: Hash,
    block_id: Hash,
    recipients: Vec<Pubkey>,
}

/// Execute in a sibling bank, never inserted into the live, slot-keyed BankForks.
fn build_alternate_block(
    source: &Bank,
    mut footer: BlockFooterV1,
    plan: &AlternateBlockPlan,
    keypair: &Keypair,
    shred_version: u16,
    parent_merkle_root: Hash,
) -> std::result::Result<AlternateBlock, String> {
    if plan.lamports == 0 || plan.recipient == plan.payer.pubkey() || plan.recipients.is_empty() {
        return Err(
            "plan requires a nonzero transfer to another account and TVU recipients".into(),
        );
    }
    if !source.is_frozen() || !source.is_alpenglow() || source.leader_id() != &keypair.pubkey() {
        return Err("expected a frozen Alpenglow bank produced by this leader".into());
    }
    let parent = source.parent().ok_or("source parent was pruned")?;
    if !parent.is_alpenglow() {
        return Err(
            "equivocation requires an Alpenglow parent (migration boundary unsupported)".into(),
        );
    }
    let parent_block_id = parent.block_id().ok_or("parent has no block id")?;
    let _parent_guard = parent
        .try_enter_transaction_execution()
        .ok_or("parent was retired")?;
    let mut bank = Bank::new_from_parent(parent.clone(), *source.leader(), source.slot());

    // Accounts are isolated by BankId, but transaction statuses are keyed by slot.
    // Preserve parent history while excluding any results from the original block.
    let mut status_cache = parent.status_cache.read().unwrap().clone();
    status_cache.clear_slot_entries(source.slot());
    bank.status_cache = Arc::new(RwLock::new(status_cache));

    // Limit the helper to native transfers: deploying programs in a sibling bank
    // would additionally require isolation of the shared program cache.
    let tx = solana_system_transaction::transfer(
        plan.payer.as_ref(),
        &plan.recipient,
        plan.lamports,
        parent.last_blockhash(),
    );
    bank.process_transaction(&tx)
        .map_err(|err| err.to_string())?;
    let entry = Entry::new(&parent.last_blockhash(), 1, vec![tx]);
    let tick = Entry::new(&entry.hash, 1, vec![]);

    let reward_cert = ValidatedRewardCert::try_new(
        &bank,
        shred_version,
        &footer.skip_reward_cert,
        &footer.notar_reward_cert,
    )
    .map_err(|err| err.to_string())?;
    let final_cert = footer
        .block_final_cert
        .clone()
        .map(|cert| ValidatedBlockFinalizationCert::try_from_footer(cert, &bank, shred_version))
        .transpose()
        .map_err(|err| err.to_string())?;
    BlockComponentProcessor::update_bank_with_footer_fields(
        &bank,
        i64::try_from(footer.block_producer_time_nanos).map_err(|err| err.to_string())?,
        None,
        reward_cert,
        final_cert.as_ref().map(|cert| cert.vote_rewards_input()),
    )
    .map_err(|err| err.to_string())?;
    bank.set_tick_height(bank.max_tick_height() - 1);
    bank.register_tick_for_test(&tick.hash);
    bank.freeze();
    footer.bank_hash = bank.hash();
    if footer.bank_hash == source.hash() {
        return Err("alternate transaction set produced the original bank hash".into());
    }

    let components = [
        BlockComponent::new_block_header(parent.slot(), parent_block_id),
        BlockComponent::EntryBatch(vec![entry]),
        BlockComponent::new_block_marker(VersionedBlockMarker::from_block_footer(footer)),
        BlockComponent::EntryBatch(vec![tick]),
    ];
    let mut shreds = Vec::new();
    let mut merkle_root = parent_merkle_root;
    let (mut next_data, mut next_code) = (0, 0);
    let mut stats = ProcessShredsStats::default();
    for (index, component) in components.iter().enumerate() {
        let is_last = index == components.len() - 1;
        let reference_tick = if is_last {
            bank.ticks_per_slot() as u8
        } else {
            0
        };
        let batch = Shredder::new(bank.slot(), parent.slot(), reference_tick, shred_version)
            .map_err(|err| err.to_string())?
            .make_merkle_shreds_from_component(
                keypair,
                component,
                is_last,
                merkle_root,
                next_data,
                next_code,
                &mut stats,
            );
        for shred in &batch {
            let next = if shred.is_data() {
                &mut next_data
            } else {
                &mut next_code
            };
            *next = (*next).max(shred.index() + 1);
        }
        merkle_root = batch.last().unwrap().merkle_root().unwrap();
        shreds.extend(batch);
    }
    if next_data > bank.max_data_shreds_per_slot() || next_code > bank.max_code_shreds_per_slot() {
        return Err("alternate block exceeds the slot's shred limits".into());
    }
    // Use the same block identity as standard broadcast and informed repair:
    // ordered FEC roots followed by the parent information and FEC count.
    let fec_roots: Vec<_> = shreds
        .iter()
        .unique_by(|shred| shred.fec_set_index())
        .sorted_unstable_by_key(|shred| shred.fec_set_index())
        .map(|shred| shred.merkle_root())
        .collect();
    let fec_count = u32::try_from(fec_roots.len()).map_err(|err| err.to_string())?;
    let parent_leaf = hashv(&[
        &parent.slot().to_le_bytes(),
        parent_block_id.as_bytes(),
        &fec_count.to_le_bytes(),
    ]);
    let tree = MerkleTree::try_new_with_len(
        fec_roots
            .into_iter()
            .chain(std::iter::once(Ok(parent_leaf))),
        fec_count as usize + 1,
    )
    .map_err(|err| err.to_string())?;
    Ok(AlternateBlock {
        shreds,
        bank_hash: bank.hash(),
        block_id: *tree.root(),
        recipients: plan.recipients.clone(),
    })
}

/// Wrap standard broadcast with a separate queue for complete alternate blocks.
/// No alternate shreds or bank IDs enter the original block's record/voting path.
#[derive(Clone)]
pub(super) struct EquivocationBroadcastRun {
    standard: StandardBroadcastRun,
    schedule: EquivocationSchedule,
    carryover: Option<WorkingBankMessage>,
    footer: Option<(Slot, BlockFooterV1)>,
    shred_version: u16,
    migration_status: Arc<agave_votor_messages::migration::MigrationStatus>,
    alternate_sender: Sender<AlternateBlock>,
    alternate_receiver: Receiver<AlternateBlock>,
}

impl EquivocationBroadcastRun {
    pub(super) fn new(
        shred_version: u16,
        schedule: EquivocationSchedule,
        migration_status: Arc<agave_votor_messages::migration::MigrationStatus>,
        votor_event_sender: VotorEventSender,
        leader_schedule_cache: Arc<LeaderScheduleCache>,
    ) -> Self {
        let (alternate_sender, alternate_receiver) = bounded(1);
        Self {
            standard: StandardBroadcastRun::new(
                shred_version,
                migration_status.clone(),
                votor_event_sender,
                leader_schedule_cache,
            ),
            schedule,
            carryover: None,
            footer: None,
            shred_version,
            migration_status,
            alternate_sender,
            alternate_receiver,
        }
    }
}

impl BroadcastRun for EquivocationBroadcastRun {
    fn run<'db>(
        &mut self,
        keypair: &Keypair,
        blockstore: &'db Blockstore,
        pinnable_slice: &mut DBPinnableSlice<'db>,
        write_batch: &mut WriteBatch,
        receiver: &Receiver<WorkingBankMessage>,
        socket_sender: &Sender<(Arc<Vec<Shred>>, Option<BroadcastShredBatchInfo>)>,
        blockstore_sender: &Sender<(Arc<Vec<Shred>>, Option<BroadcastShredBatchInfo>)>,
    ) -> Result<()> {
        let mut stats = ProcessShredsStats::default();
        let received =
            broadcast_utils::recv_slot_components(receiver, &mut self.carryover, &mut stats)?;
        let bank = received.bank.clone();
        let slot = bank.slot();
        if self
            .footer
            .as_ref()
            .is_some_and(|(footer_slot, _)| *footer_slot != slot)
        {
            self.footer = None;
        }
        if self.schedule.contains_key(&slot)
            && let BroadcastItem::Component(component) = &received.item
            && let Some(VersionedBlockMarker::V1(marker)) = component.as_marker()
            && let Some(VersionedBlockFooter::V1(footer)) = marker.as_block_footer()
        {
            self.footer = Some((slot, footer.clone()));
        }
        let is_last = received.last_tick_height == bank.max_tick_height();
        let produced_sender = self
            .schedule
            .get(&slot)
            .and_then(|plan| plan.produced_sender.clone());
        // Build before the standard path publishes the completed block to Votor,
        // which could otherwise root it and detach its parent. Still finish the
        // original block if the development plan fails.
        let alternate = is_last.then(|| self.schedule.remove(&slot)).flatten().map(
            |plan| -> Result<AlternateBlock> {
                if self
                    .migration_status
                    .genesis_block()
                    .is_some_and(|genesis| genesis.slot != 0 && genesis.slot == bank.parent_slot())
                {
                    return Err(Error::Equivocation("migration boundary unsupported".into()));
                }
                let footer = self
                    .footer
                    .take()
                    .filter(|(s, _)| *s == slot)
                    .ok_or_else(|| {
                        Error::Equivocation(format!("slot {slot} has no Alpenglow footer"))
                    })?
                    .1;
                let parent_root = match broadcast_utils::get_chained_merkle_root_from_parent(
                    slot,
                    bank.parent_slot(),
                    blockstore,
                ) {
                    Ok(root) => root,
                    Err(Error::UnknownLastIndex(_)) | Err(Error::UnknownSlotMeta(_)) => bank
                        .parent()
                        .and_then(|parent| parent.block_id())
                        .ok_or_else(|| Error::Equivocation("parent was pruned".into()))?
                        .to_hash(),
                    Err(err) => return Err(err),
                };
                let alternate = build_alternate_block(
                    &bank,
                    footer,
                    &plan,
                    keypair,
                    self.shred_version,
                    parent_root,
                )
                .map_err(Error::Equivocation)?;
                Ok(alternate)
            },
        );
        self.standard.process_receive_results(
            keypair,
            blockstore,
            pinnable_slice,
            write_batch,
            socket_sender,
            blockstore_sender,
            received,
            &mut stats,
        )?;
        if let Some(alternate) = alternate.transpose()? {
            // Persist before delivery so peers can repair B even if every
            // packet of its initial broadcast is lost. A stays in Original.
            blockstore.insert_trusted_alternate_shreds(
                alternate.block_id,
                &alternate.shreds,
                pinnable_slice,
                write_batch,
            )?;
            info!(
                "equivocating slot {slot}: original bank hash {}, alternate bank hash {}, \
                 alternate block id {}",
                bank.hash(),
                alternate.bank_hash,
                alternate.block_id,
            );
            if let Some(sender) = produced_sender {
                let _ = sender.try_send(EquivocationResult {
                    slot,
                    original_block_id: bank
                        .block_id()
                        .expect("standard broadcast set block ID")
                        .to_hash(),
                    original_bank_hash: bank.hash(),
                    alternate_block_id: alternate.block_id,
                    alternate_bank_hash: alternate.bank_hash,
                });
            }
            self.alternate_sender
                .send(alternate)
                .map_err(|_| Error::Send)?;
        }
        Ok(())
    }

    fn transmit(
        &mut self,
        receiver: &TransmitReceiver,
        cluster_info: &ClusterInfo,
        sock: BroadcastSocket,
        bank_forks: &RwLock<BankForks>,
    ) -> Result<()> {
        crossbeam_channel::select! {
            recv(receiver) -> message => {
                let (shreds, batch_info) = message?;
                if let Some(recipients) = shreds.first()
                    .and_then(|shred| self.schedule.get(&shred.slot()))
                    .and_then(|plan| plan.original_recipients.as_ref()) {
                    return send_to_recipients(&shreds, recipients, cluster_info, sock);
                }
                self.standard.broadcast(sock, cluster_info, shreds, batch_info, bank_forks)
            }
            recv(self.alternate_receiver) -> message => {
                let alternate = message?;
                send_to_recipients(&alternate.shreds, &alternate.recipients, cluster_info, sock)
            }
        }
    }

    fn record<'db>(
        &mut self,
        receiver: &RecordReceiver,
        blockstore: &'db Blockstore,
        pinnable_slice: &mut DBPinnableSlice<'db>,
        write_batch: &mut WriteBatch,
    ) -> Result<()> {
        self.standard
            .record(receiver, blockstore, pinnable_slice, write_batch)
    }
}

fn send_to_recipients(
    shreds: &[Shred],
    recipients: &[Pubkey],
    cluster_info: &ClusterInfo,
    sock: BroadcastSocket,
) -> Result<()> {
    let BroadcastSocket::Udp(sock) = sock else {
        unreachable!("scheduled equivocation requires UDP");
    };
    // Attempt each peer independently so a missing gossip address cannot
    // prevent delivery to the remaining recipients.
    let mut first_error = None;
    for pubkey in recipients {
        let result = cluster_info
            .lookup_contact_info(pubkey, |node| node.tvu(Protocol::UDP))
            .flatten()
            .filter(|addr| cluster_info.socket_addr_space().check(addr))
            .ok_or_else(|| Error::Equivocation(format!("no usable TVU address for {pubkey}")))
            .and_then(|addr| {
                batch_send(sock, shreds.iter().map(|shred| (shred.payload(), addr)))
                    .map_err(|SendPktsError::IoError(err)| Error::Io(err))
            });
        if let Err(err) = result {
            first_error.get_or_insert(err);
        }
    }
    first_error.map_or(Ok(()), Err)
}
