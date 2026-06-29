use {
    arbitrary::{Arbitrary, Unstructured},
    solana_clock::Slot,
    solana_program_runtime::{
        invoke_context::Executable,
        loaded_programs::{
            get_mock_program_runtime_environment, BlockRelation, ForkGraph, ProgramCache,
            ProgramCacheForTxBatch, ProgramCacheMatchCriteria, ProgramRuntimeEnvironment,
            ProgramToLoad,
        },
        program_cache_entry::{
            ProgramCacheEntry, ProgramCacheEntryOwner, ProgramCacheEntryType,
            DELAY_VISIBILITY_SLOT_OFFSET,
        },
        program_metrics::ProgramStatistics,
    },
    solana_pubkey::Pubkey,
    std::{
        fs::File,
        io::Read,
        ops::ControlFlow,
        sync::{atomic::AtomicU64, Arc, RwLock},
    },
};

#[derive(Arbitrary)]
pub struct FuzzData {
    forks: Vec<Vec<Slot>>,
    programs: Vec<ProgramEntry>,
    /// Slot to reroot the cache to before the visibility-preservation check.
    new_root: u8,
}

#[derive(Arbitrary)]
pub struct ProgramEntry {
    deployment_slot: u8,
    program: Program,
}

#[derive(Arbitrary)]
pub enum Program {
    One,
    Two,
    Three,
}

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        let Ok(d) = FuzzData::arbitrary(&mut Unstructured::new(data)) else {
            return;
        };
        let mut cache = ProgramCache::<TestForkGraphSpecific>::new(0);
        let env = get_mock_program_runtime_environment();

        // The blockstore is always a TREE, and extract/prune assume it. Arbitrary
        // overlapping input chains can describe a non-tree (a slot descending from
        // two unrelated slots), which makes prune and extract legitimately disagree
        // on inputs that can't occur in production. Normalize to a valid tree first.
        let forks = tree_forks(&d.forks);
        let mut fork_graph = TestForkGraphSpecific::default();
        for fork in &forks {
            fork_graph.insert_fork(fork);
        }

        let fork_graph = Arc::new(RwLock::new(fork_graph));
        cache.set_fork_graph(Arc::downgrade(&fork_graph));

        let program1 = Pubkey::new_from_array([11u8; 32]);
        let program2 = Pubkey::new_from_array([22u8; 32]);
        let program3 = Pubkey::new_from_array([33u8; 32]);
        for program in d.programs {
            cache.assign_program(
                &env,
                match program.program {
                    Program::One => program1,
                    Program::Two => program2,
                    Program::Three => program3,
                },
                program.deployment_slot as u64,
                new_test_entry(
                    program.deployment_slot as u64,
                    program.deployment_slot as u64 + DELAY_VISIBILITY_SLOT_OFFSET,
                ),
            );
        }

        // --- Prune visibility-preservation differential ---------------------
        //
        // Rerooting must never change what a still-live fork sees: prune only
        // drops entries on orphaned branches and redundant older ancestors, so
        // for any slot that survives the reroot, `extract` must return exactly
        // the same thing before and after. We snapshot extract over the live
        // slots, prune, re-extract, and assert equality.
        let programs = [program1, program2, program3];
        // Root advances monotonically (prune debug_asserts latest_root_slot <= new_root).
        let new_root = cache.latest_root_slot.max(d.new_root as Slot);

        // Live slots = candidate slots (drawn from the fork topology) that are
        // descended from, or equal to, the new root. Anything Unrelated/Ancestor
        // to new_root is gone after the reroot and isn't queried.
        let live_slots: Vec<Slot> = {
            let fg = fork_graph.read().unwrap();
            distinct_slots(&forks)
                .into_iter()
                .filter(|s| {
                    matches!(
                        fg.relationship(*s, new_root),
                        BlockRelation::Equal | BlockRelation::Descendant
                    )
                })
                .collect()
        };

        let before = visibility_snapshot(&cache, &programs, &live_slots, &env);
        {
            let fg = fork_graph.read().unwrap();
            cache.prune(new_root, None, &fg);
        }
        let after = visibility_snapshot(&cache, &programs, &live_slots, &env);

        assert_eq!(
            before, after,
            "prune(new_root={new_root}) changed what a still-live fork sees"
        );
    });
}

/// Normalizes arbitrary input chains into a valid fork tree rooted at slot 0.
///
/// A slot must have a single, fixed ancestry (the blockstore is a tree), so each
/// non-root slot is placed exactly once — its first-seen chain wins; later chains
/// reusing it skip it. Every chain is rooted at slot 0 (the cache's initial root),
/// yielding a root with independent branches: a tree in which no slot can descend
/// from two unrelated slots.
fn tree_forks(input: &[Vec<Slot>]) -> Vec<Vec<Slot>> {
    let mut placed = std::collections::HashSet::from([0u64]);
    let mut out = Vec::new();
    for chain in input {
        let mut sorted = chain.clone();
        sorted.sort_unstable();
        sorted.dedup();
        let mut path = vec![0u64];
        path.extend(sorted.into_iter().filter(|&s| placed.insert(s)));
        if path.len() > 1 {
            out.push(path);
        }
    }
    out
}

/// Distinct slots appearing in the fork topology — the candidate query slots.
fn distinct_slots(forks: &[Vec<Slot>]) -> Vec<Slot> {
    let mut slots: Vec<Slot> = forks.iter().flatten().copied().collect();
    slots.sort_unstable();
    slots.dedup();
    slots
}

/// Compact identity of an extracted entry: (deployment, effective, is_delay_vis),
/// or `None` for a miss. Enough to detect any change in visibility.
type Visible = Option<(Slot, Slot, bool)>;

/// For each (live slot, program), records what `extract` returns. Read-only: the
/// usage-counter and hit/miss flags are passed `false` so the probe can't perturb
/// the cache state it's measuring.
fn visibility_snapshot(
    cache: &ProgramCache<TestForkGraphSpecific>,
    programs: &[Pubkey],
    live_slots: &[Slot],
    env: &ProgramRuntimeEnvironment,
) -> Vec<Visible> {
    let mut out = Vec::with_capacity(live_slots.len().saturating_mul(programs.len()));
    for &slot in live_slots {
        let mut search: Vec<ProgramToLoad> = programs
            .iter()
            .map(|p| ProgramToLoad {
                program_id: p,
                loader: ProgramCacheEntryOwner::LoaderV2, // matches new_test_entry's owner
                match_criteria: ProgramCacheMatchCriteria::NoCriteria,
                last_modification_slot: slot,
            })
            .collect();
        let mut batch = ProgramCacheForTxBatch::new(slot);
        cache.extract(&mut search, &mut batch, env, false, false);
        for p in programs {
            out.push(batch.find(p).map(|e| {
                (
                    e.deployment_slot,
                    e.effective_slot,
                    matches!(e.program, ProgramCacheEntryType::DelayVisibility),
                )
            }));
        }
    }
    out
}

#[derive(Default)]
struct TestForkGraphSpecific {
    forks: Vec<Vec<Slot>>,
}

impl TestForkGraphSpecific {
    fn insert_fork(&mut self, fork: &[Slot]) {
        let mut fork = fork.to_vec();
        fork.sort();
        self.forks.push(fork)
    }
}

impl ForkGraph for TestForkGraphSpecific {
    fn relationship(&self, a: Slot, b: Slot) -> BlockRelation {
        match self.forks.iter().try_for_each(|fork| {
            let relation = fork
                .iter()
                .position(|x| *x == a)
                .and_then(|a_pos| {
                    fork.iter().position(|x| *x == b).and_then(|b_pos| {
                        (a_pos == b_pos)
                            .then_some(BlockRelation::Equal)
                            .or_else(|| (a_pos < b_pos).then_some(BlockRelation::Ancestor))
                            .or(Some(BlockRelation::Descendant))
                    })
                })
                .unwrap_or(BlockRelation::Unrelated);

            if relation != BlockRelation::Unrelated {
                return ControlFlow::Break(relation);
            }

            ControlFlow::Continue(())
        }) {
            ControlFlow::Break(relation) => relation,
            _ => BlockRelation::Unrelated,
        }
    }
}

fn new_test_entry(deployment_slot: Slot, effective_slot: Slot) -> Arc<ProgramCacheEntry> {
    new_test_entry_with_usage(
        deployment_slot,
        effective_slot,
        ProgramStatistics::default(),
    )
}
pub(crate) fn new_test_entry_with_usage(
    deployment_slot: Slot,
    effective_slot: Slot,
    stats: ProgramStatistics,
) -> Arc<ProgramCacheEntry> {
    Arc::new(ProgramCacheEntry {
        program: new_loaded_entry(get_mock_program_runtime_environment()),
        account_owner: ProgramCacheEntryOwner::LoaderV2,
        account_size: 0,
        deployment_slot,
        effective_slot,
        stats: Arc::new(stats),
        latest_access_slot: AtomicU64::new(deployment_slot),
    })
}

fn new_loaded_entry(env: ProgramRuntimeEnvironment) -> ProgramCacheEntryType {
    let mut elf = Vec::new();
    File::open("../../programs/bpf_loader/test_elfs/out/noop_aligned.so")
        .unwrap()
        .read_to_end(&mut elf)
        .unwrap();
    let executable = Executable::load(&elf, Arc::clone(&*env)).unwrap();
    ProgramCacheEntryType::Loaded(executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the prune visibility-preservation differential on a real fork
    /// tree (so `live_slots` is non-empty and has hits — not a vacuous pass),
    /// and confirms a correct reroot leaves live-fork visibility unchanged.
    /// `tree_forks` must give every non-root slot a single ancestry (place it in
    /// exactly one fork), even when input chains overlap inconsistently — the
    /// property that prevents the non-tree divergence the fuzzer found.
    #[test]
    fn tree_forks_yields_unique_slot_ancestry() {
        // Slot 5 appears under two unrelated prefixes (0 and 1): a non-tree input.
        let forks = tree_forks(&[vec![0, 5], vec![1, 5]]);
        let mut owner = std::collections::HashMap::new();
        for (i, f) in forks.iter().enumerate() {
            for &s in f.iter().filter(|&&s| s != 0) {
                assert!(owner.insert(s, i).is_none(), "slot {s} placed in two forks");
            }
        }
    }

    #[test]
    fn prune_preserves_live_visibility() {
        // Two branches sharing trunk slot 20: 20->30 (branch A) and 20->40 (B).
        let mut fg = TestForkGraphSpecific::default();
        fg.insert_fork(&[10, 20, 30]);
        fg.insert_fork(&[10, 20, 40]);
        let forks = vec![vec![10, 20, 30], vec![10, 20, 40]];
        let fg = Arc::new(RwLock::new(fg));

        let mut cache = ProgramCache::<TestForkGraphSpecific>::new(0);
        cache.set_fork_graph(Arc::downgrade(&fg));
        let env = get_mock_program_runtime_environment();

        let prog = Pubkey::new_from_array([11u8; 32]);
        // On trunk (visible everywhere) and on branch A (visible only under 30).
        cache.assign_program(&env, prog, 20, new_test_entry(20, 21));
        cache.assign_program(&env, prog, 30, new_test_entry(30, 31));
        // Orphan w.r.t. a reroot to 30: deployed only on branch B.
        cache.assign_program(&env, prog, 40, new_test_entry(40, 41));

        // Reroot to 30: live = {30}; slots 40 (sibling) and 10/20 (ancestors) die.
        let new_root = 30;
        let live: Vec<Slot> = {
            let g = fg.read().unwrap();
            distinct_slots(&forks)
                .into_iter()
                .filter(|s| {
                    matches!(
                        g.relationship(*s, new_root),
                        BlockRelation::Equal | BlockRelation::Descendant
                    )
                })
                .collect()
        };
        assert_eq!(live, vec![30], "expected slot 30 to be the only live slot");

        let programs = [prog];
        let before = visibility_snapshot(&cache, &programs, &live, &env);
        assert!(
            before.iter().any(Option::is_some),
            "probe should hit a program"
        );

        {
            let g = fg.read().unwrap();
            cache.prune(new_root, None, &g);
        }
        let after = visibility_snapshot(&cache, &programs, &live, &env);

        assert_eq!(before, after, "reroot changed live-fork visibility");
    }
}
