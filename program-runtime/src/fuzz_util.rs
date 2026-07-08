//! Helpers exposed under the `fuzz` feature so the out-of-tree ziggy harness in
//! `program-runtime/fuzz/` can build cache entries and a mock fork graph without
//! reaching into this crate's `#[cfg(test)]` internals.
//!
//! These intentionally mirror the private mocks in the `loaded_programs` test
//! module (`TestForkGraphSpecific`, `new_test_entry`, `set_tombstone`,
//! `get_mock_program_runtime_environment`), but are reachable from a separate
//! crate. Keep them in sync if those mocks change semantics.

use {
    crate::{
        loaded_programs::{BlockRelation, ForkGraph, ProgramRuntimeEnvironment},
        program_cache_entry::{ProgramCacheEntry, ProgramCacheEntryOwner, ProgramCacheEntryType},
    },
    percentage::PercentageInteger,
    solana_clock::Slot,
    solana_sbpf::{elf::Executable, program::BuiltinProgram},
    solana_svm_type_overrides::sync::{Arc, atomic::AtomicU64},
    std::sync::LazyLock,
};

/// Number of distinct mock runtime environments the harness can choose from.
pub const NUM_ENVIRONMENTS: usize = 2;

/// A small fixed pool of distinct mock runtime environments.
///
/// Equality of [`ProgramRuntimeEnvironment`] is by `Arc` pointer identity, so
/// the pool is built once and we hand out clones (which preserve identity). Two
/// environments let the harness exercise the environment-mismatch branches of
/// `extract`/`prune`.
static ENVIRONMENTS: LazyLock<[ProgramRuntimeEnvironment; NUM_ENVIRONMENTS]> =
    LazyLock::new(|| {
        [
            ProgramRuntimeEnvironment::from(BuiltinProgram::new_mock()),
            ProgramRuntimeEnvironment::from(BuiltinProgram::new_mock()),
        ]
    });

/// Returns one of the mock environments (index is clamped into the pool).
pub fn mock_environment(env_id: usize) -> ProgramRuntimeEnvironment {
    let envs = &*ENVIRONMENTS;
    match envs.get(env_id) {
        Some(env) => env.clone(),
        None => envs.first().expect("non-empty env pool").clone(),
    }
}

/// The ELF backing `Loaded` entries, read once. `CARGO_MANIFEST_DIR` resolves at
/// compile time to the `program-runtime` crate directory, so the path is stable
/// no matter what working directory the fuzzer runs from.
static NOOP_ELF: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../programs/bpf_loader/test_elfs/out/noop_aligned.so"
    );
    std::fs::read(path).expect("failed to read noop ELF for fuzz Loaded entries")
});

/// The kind of payload to place in a fabricated [`ProgramCacheEntry`].
#[derive(Clone, Copy, Debug)]
pub enum FuzzEntryKind {
    Loaded,
    Unloaded,
    Builtin,
    TombstoneClosed,
    TombstoneFailedVerification,
}

/// Maps a small integer to a loader owner.
pub fn owner_from_u8(v: u8) -> ProgramCacheEntryOwner {
    match v {
        0 => ProgramCacheEntryOwner::NativeLoader,
        1 => ProgramCacheEntryOwner::LoaderV1,
        2 => ProgramCacheEntryOwner::LoaderV2,
        3 => ProgramCacheEntryOwner::LoaderV3,
        _ => ProgramCacheEntryOwner::LoaderV4,
    }
}

/// Builds a [`PercentageInteger`] (clamped to 0..=100) for eviction calls.
pub fn percent(p: u8) -> PercentageInteger {
    let clamped = if p > 100 { 100 } else { p };
    percentage::Percentage::from(clamped)
}

/// Fabricates a cache entry of the requested kind. `Loaded` entries reuse the
/// noop ELF with no verification or JIT (matching the test helper), keeping
/// per-entry construction cheap.
pub fn make_entry(
    kind: FuzzEntryKind,
    deployment_slot: Slot,
    effective_slot: Slot,
    env_id: usize,
    owner: ProgramCacheEntryOwner,
) -> Arc<ProgramCacheEntry> {
    let env = mock_environment(env_id);
    let program = match kind {
        FuzzEntryKind::Loaded => {
            let executable =
                Executable::load(&NOOP_ELF, Arc::clone(&*env)).expect("noop ELF should load");
            ProgramCacheEntryType::Loaded(executable)
        }
        FuzzEntryKind::Unloaded => ProgramCacheEntryType::Unloaded(env),
        FuzzEntryKind::Builtin => ProgramCacheEntryType::Builtin(BuiltinProgram::new_mock()),
        FuzzEntryKind::TombstoneClosed => ProgramCacheEntryType::Closed,
        FuzzEntryKind::TombstoneFailedVerification => {
            ProgramCacheEntryType::FailedVerification(env)
        }
    };
    Arc::new(ProgramCacheEntry {
        program,
        account_owner: owner,
        account_size: 0,
        deployment_slot,
        effective_slot,
        stats: Arc::default(),
        latest_access_slot: AtomicU64::new(deployment_slot),
    })
}

/// Fork-graph mock identical in behaviour to the test module's
/// `TestForkGraphSpecific`: each inserted fork is a slot chain in which an
/// earlier position is an ancestor of a later one.
#[derive(Default)]
pub struct MockForkGraph {
    forks: Vec<Vec<Slot>>,
}

impl MockForkGraph {
    /// Registers a fork as a chain of slots (ancestor first after sorting).
    pub fn insert_fork(&mut self, fork: &[Slot]) {
        let mut fork = fork.to_vec();
        fork.sort_unstable();
        self.forks.push(fork);
    }
}

impl ForkGraph for MockForkGraph {
    fn relationship(&self, a: Slot, b: Slot) -> BlockRelation {
        for fork in self.forks.iter() {
            let a_pos = fork.iter().position(|x| *x == a);
            let b_pos = fork.iter().position(|x| *x == b);
            if let (Some(a_pos), Some(b_pos)) = (a_pos, b_pos) {
                return match a_pos.cmp(&b_pos) {
                    std::cmp::Ordering::Equal => BlockRelation::Equal,
                    std::cmp::Ordering::Less => BlockRelation::Ancestor,
                    std::cmp::Ordering::Greater => BlockRelation::Descendant,
                };
            }
        }
        BlockRelation::Unrelated
    }
}
