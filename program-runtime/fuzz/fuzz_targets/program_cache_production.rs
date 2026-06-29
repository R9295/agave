use {
    arbitrary::{Arbitrary, Unstructured}, solana_clock::Slot, solana_program_runtime::{invoke_context::Executable, loaded_programs::{
            BlockRelation, ForkGraph, ProgramCache, ProgramRuntimeEnvironment, get_mock_program_runtime_environment
        }, program_cache_entry::{DELAY_VISIBILITY_SLOT_OFFSET, ProgramCacheEntry, ProgramCacheEntryOwner, ProgramCacheEntryType}, program_metrics::ProgramStatistics}, solana_pubkey::Pubkey, std::{fs::File, io::Read, ops::ControlFlow, sync::{Arc, RwLock, atomic::AtomicU64}}
};

#[derive(Arbitrary)]
pub struct FuzzData {
    forks: Vec<Vec<Slot>>,
    programs: Vec<ProgramEntry>,
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

        let mut fork_graph = TestForkGraphSpecific::default();
        for fork in d.forks {
            let cloned = fork.clone();
            // cloned.insert(0, Slot::from(0u32));
            fork_graph.insert_fork(&cloned);
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
    });
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
