use {
    arbitrary::{Arbitrary, Unstructured},
    solana_clock::Slot,
    solana_program_runtime::{
        fuzz_util::{make_entry, mock_environment, FuzzEntryKind, NUM_ENVIRONMENTS},
        loaded_programs::{
            BlockRelation, ForkGraph, ProgramCache, ProgramCacheForTxBatch,
            ProgramCacheMatchCriteria, ProgramRuntimeEnvironment, ProgramToLoad,
        },
        program_cache_entry::{
            ProgramCacheEntryOwner, ProgramCacheEntryType, DELAY_VISIBILITY_SLOT_OFFSET,
        },
    },
    solana_pubkey::Pubkey,
    std::{
        ops::ControlFlow,
        sync::{Arc, RwLock},
    },
};

// The `#[cfg(test)]` entry builders load the ELF directly and use the singleton
// mock env; the fuzz body drives everything through `fuzz_util` instead.
#[cfg(test)]
use {
    solana_program_runtime::{
        invoke_context::Executable, loaded_programs::get_mock_program_runtime_environment,
        program_cache_entry::ProgramCacheEntry, program_metrics::ProgramStatistics,
    },
    std::{fs::File, io::Read, sync::atomic::AtomicU64},
};

#[derive(Arbitrary)]
pub struct FuzzData {
    /// Slots are drawn as `u8` so they share a range with the reroot indices —
    /// this lets a reroot actually land on a slot partway up the tree and orphan
    /// branches, instead of `u64` slots that a small index could never match.
    forks: Vec<Vec<u8>>,
    /// Interleaved deploy/reroot operations applied in order. Modelling a sequence
    /// (rather than "deploy all, prune once") is what advances `latest_root_slot`
    /// across successive prunes, exercising the incremental-reroot logic — e.g. the
    /// `deployment_slot <= latest_root_slot` retention arm that a single prune from
    /// root 0 can never reach.
    ops: Vec<Op>,
}

#[derive(Arbitrary)]
pub enum Op {
    /// Deploy a program version at a tree slot.
    Assign {
        /// Index (mod tree size) selecting the deployment slot from the tree's
        /// actual slots, so the entry lands on a real branch instead of an off-tree
        /// slot that prune/extract only ever see as `Unrelated`.
        deployment_slot_idx: u8,
        /// Which mock runtime environment (mod `NUM_ENVIRONMENTS`) this version is
        /// compiled for. Mixing environments is the only way to reach prune's
        /// older-epoch retention branch (keep a divergent-env ancestor) and
        /// extract's env-mismatch skip. Env 0 is the execution env used to query,
        /// so env 1 entries are the "different epoch" ones.
        env_id: u8,
        /// Payload kind. Tombstones/`Unloaded`/`Builtin` drive extract's early-out
        /// paths (e.g. `Unloaded` → break) and prune's type/env handling, which a
        /// `Loaded`-only harness never reaches. Note assign_program only permits
        /// `Unloaded`→`Loaded` and `Builtin`→`Builtin` replacements at a colliding
        /// (slot, owner, env); other same-slot collisions take the "unexpected
        /// replacement" path (a no-op drop, not a panic — the debug_assert is off).
        kind: EntryKind,
        program: Program,
    },
    /// Reroot the cache and assert the prune preserves live-fork visibility.
    Reroot {
        /// Index (mod tree size) selecting the reroot target from the tree's actual
        /// slots. prune keys every decision on `relationship(deployment_slot,
        /// new_root)`, so a `new_root` that is a real interior node is what orphans
        /// sibling branches; an off-tree value would just be `Unrelated` to all.
        new_root_idx: u8,
    },
}

#[derive(Arbitrary)]
pub enum Program {
    One,
    Two,
    Three,
}

/// Fuzzer-facing mirror of `fuzz_util::FuzzEntryKind` (which isn't `Arbitrary`).
#[derive(Arbitrary)]
pub enum EntryKind {
    Loaded,
    Unloaded,
    Builtin,
    Closed,
    FailedVerification,
}

impl From<EntryKind> for FuzzEntryKind {
    fn from(k: EntryKind) -> Self {
        match k {
            EntryKind::Loaded => FuzzEntryKind::Loaded,
            EntryKind::Unloaded => FuzzEntryKind::Unloaded,
            EntryKind::Builtin => FuzzEntryKind::Builtin,
            EntryKind::Closed => FuzzEntryKind::TombstoneClosed,
            EntryKind::FailedVerification => FuzzEntryKind::TombstoneFailedVerification,
        }
    }
}

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        let Ok(d) = FuzzData::arbitrary(&mut Unstructured::new(data)) else {
            return;
        };
        let mut cache = ProgramCache::<TestForkGraphSpecific>::new(0);
        // Execution environment used to *query* the cache (assign + extract). It is
        // env 0 of the mock pool, so entries deployed with env 1 are seen as
        // belonging to a different epoch — that mismatch is what drives prune's
        // divergent-env retention and extract's env-skip logic.
        let env = mock_environment(0);

        // The blockstore is always a TREE, and extract/prune assume it. Arbitrary
        // overlapping input chains can describe a non-tree (a slot descending from
        // two unrelated slots), which makes prune and extract legitimately disagree
        // on inputs that can't occur in production. Normalize to a valid tree first.
        let forks = tree_forks(&d.forks);
        let mut fork_graph = TestForkGraphSpecific::default();
        for fork in &forks {
            fork_graph.insert_fork(fork);
        }

        // Candidate slots deployments and reroots are drawn from: the tree's actual
        // nodes. Always includes root slot 0, so it's non-empty even with no forks.
        let tree_slots = {
            let mut s = distinct_slots(&forks);
            if s.is_empty() {
                s.push(0);
            }
            s
        };

        let fork_graph = Arc::new(RwLock::new(fork_graph));
        cache.set_fork_graph(Arc::downgrade(&fork_graph));

        let program1 = Pubkey::new_from_array([11u8; 32]);
        let program2 = Pubkey::new_from_array([22u8; 32]);
        let program3 = Pubkey::new_from_array([33u8; 32]);
        let programs = [program1, program2, program3];

        let debug = std::env::var("AFL_DEBUG").as_deref() == Ok("1");
        let named = [
            ("program1", program1),
            ("program2", program2),
            ("program3", program3),
        ];

        // Apply the deploy/reroot operations in order. Reroots advance
        // `latest_root_slot`, so each subsequent prune builds on the previous one.
        for op in d.ops {
            match op {
                Op::Assign {
                    deployment_slot_idx,
                    env_id,
                    kind,
                    program,
                } => {
                    let deployment_slot =
                        tree_slots[deployment_slot_idx as usize % tree_slots.len()];
                    let entry = make_entry(
                        kind.into(),
                        deployment_slot,
                        deployment_slot + DELAY_VISIBILITY_SLOT_OFFSET,
                        env_id as usize % NUM_ENVIRONMENTS,
                        ProgramCacheEntryOwner::LoaderV2, // probe matches this owner
                    );
                    cache.assign_program(
                        &env,
                        match program {
                            Program::One => program1,
                            Program::Two => program2,
                            Program::Three => program3,
                        },
                        deployment_slot,
                        entry,
                    );
                }
                Op::Reroot { new_root_idx } => {
                    // --- Prune visibility-preservation differential -------------
                    //
                    // Rerooting must never change what a still-live fork sees:
                    // prune only drops entries on orphaned branches and redundant
                    // older ancestors, so for any slot that survives the reroot,
                    // `extract` must return exactly the same thing before and after.
                    //
                    // Reroot to a real tree node so the prune lands on the topology.
                    // Clamp monotonically upward (prune debug_asserts
                    // latest_root_slot <= new_root).
                    let new_root = cache
                        .latest_root_slot
                        .max(tree_slots[new_root_idx as usize % tree_slots.len()]);

                    // Live slots = tree slots descended from, or equal to, the new
                    // root. Anything Unrelated/Ancestor to new_root is gone after
                    // the reroot and isn't queried.
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

                    if debug {
                        eprintln!("=== reroot to new_root={new_root} ===");
                        print_fork_tree(&forks, new_root, &live_slots);
                        dump_cache(&cache, &named, "before");
                    }

                    let before = visibility_snapshot(&cache, &programs, &live_slots, &env);
                    {
                        let fg = fork_graph.read().unwrap();
                        cache.prune(new_root, None, &fg);
                    }
                    let after = visibility_snapshot(&cache, &programs, &live_slots, &env);

                    if debug {
                        dump_cache(&cache, &named, "after");
                    }

                    assert_eq!(
                        before, after,
                        "prune(new_root={new_root}) changed what a still-live fork sees"
                    );
                }
            }
        }
    });
}

/// Normalizes arbitrary input chains into a valid fork tree rooted at slot 0.
///
/// A slot must have a single, fixed ancestry (the blockstore is a tree), so each
/// non-root slot is placed exactly once — its first-seen chain wins; later chains
/// reusing it skip it. Every chain is rooted at slot 0 (the cache's initial root),
/// yielding a root with independent branches: a tree in which no slot can descend
/// from two unrelated slots.
fn tree_forks(input: &[Vec<u8>]) -> Vec<Vec<Slot>> {
    let mut placed = std::collections::HashSet::from([0u64]);
    let mut out = Vec::new();
    for chain in input {
        let mut sorted: Vec<Slot> = chain.iter().map(|&s| s as Slot).collect();
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

/// Dumps every entry currently held in the cache, grouped by program, so the
/// effect of `prune` can be seen: run before and after and diff which
/// (deployment_slot, effective_slot) versions disappeared. `label` distinguishes
/// the two dumps ("before" / "after").
fn dump_cache(
    cache: &ProgramCache<TestForkGraphSpecific>,
    programs: &[(&str, Pubkey)],
    label: &str,
) {
    eprintln!("cache {label} prune:");
    for (name, id) in programs {
        let versions = cache.get_slot_versions_for_tests(id);
        if versions.is_empty() {
            continue;
        }
        let exec_env = mock_environment(0);
        let mut rendered: Vec<String> = versions
            .iter()
            .map(|e| {
                let kind = match e.program {
                    ProgramCacheEntryType::Loaded(_) => "loaded",
                    ProgramCacheEntryType::DelayVisibility => "delay-vis",
                    ProgramCacheEntryType::Unloaded(_) => "unloaded",
                    ProgramCacheEntryType::FailedVerification(_) => "failed-verify",
                    ProgramCacheEntryType::Closed => "closed",
                    ProgramCacheEntryType::Builtin(_) => "builtin",
                };
                // env0 = the execution env we query with; env1 = a divergent epoch.
                let env = match e.program.get_environment() {
                    None => "no-env",
                    Some(env) if *env == exec_env => "env0",
                    Some(_) => "env1",
                };
                format!(
                    "deploy@{} effective@{} [{kind} {env}]",
                    e.deployment_slot, e.effective_slot
                )
            })
            .collect();
        rendered.sort();
        eprintln!("  {name}: {}", rendered.join(", "));
    }
}

/// Renders the normalized fork forest as an ASCII tree rooted at slot 0, so a
/// reroot can be reasoned about visually. `new_root` is marked `<-- new_root` and
/// live (surviving) slots are tagged `*`.
fn print_fork_tree(forks: &[Vec<Slot>], new_root: Slot, live_slots: &[Slot]) {
    use std::collections::{BTreeMap, BTreeSet};

    // Reconstruct parent->children from the chains (each chain is a path).
    let mut children: BTreeMap<Slot, BTreeSet<Slot>> = BTreeMap::new();
    let mut all: BTreeSet<Slot> = BTreeSet::from([0]);
    for fork in forks {
        for pair in fork.windows(2) {
            children.entry(pair[0]).or_default().insert(pair[1]);
            all.insert(pair[0]);
            all.insert(pair[1]);
        }
    }
    let live: BTreeSet<Slot> = live_slots.iter().copied().collect();

    fn walk(
        slot: Slot,
        prefix: &str,
        is_last: bool,
        is_root: bool,
        children: &BTreeMap<Slot, BTreeSet<Slot>>,
        new_root: Slot,
        live: &BTreeSet<Slot>,
    ) {
        let connector = if is_root {
            ""
        } else if is_last {
            "└── "
        } else {
            "├── "
        };
        let mut tags = String::new();
        if slot == new_root {
            tags.push_str(" <-- new_root");
        }
        if live.contains(&slot) {
            tags.push_str(" *");
        }
        eprintln!("{prefix}{connector}{slot}{tags}");

        if let Some(kids) = children.get(&slot) {
            let child_prefix = if is_root {
                String::new()
            } else {
                format!("{prefix}{}", if is_last { "    " } else { "│   " })
            };
            let n = kids.len();
            for (i, &child) in kids.iter().enumerate() {
                walk(
                    child,
                    &child_prefix,
                    i == n - 1,
                    false,
                    children,
                    new_root,
                    live,
                );
            }
        }
    }

    walk(0, "", true, true, &children, new_root, &live);
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

#[cfg(test)]
fn new_test_entry(deployment_slot: Slot, effective_slot: Slot) -> Arc<ProgramCacheEntry> {
    new_test_entry_with_usage(
        deployment_slot,
        effective_slot,
        ProgramStatistics::default(),
    )
}
#[cfg(test)]
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

#[cfg(test)]
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
