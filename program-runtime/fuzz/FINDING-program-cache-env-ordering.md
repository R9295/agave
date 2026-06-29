# Finding: environment-relative `binary_search` mis-sort in `ProgramCache::assign_program` produces duplicate / dropped cache entries

| | |
|---|---|
| **Component** | `program-runtime` — `ProgramCache` (`program-runtime/src/loaded_programs.rs`) |
| **Severity** | Low (liveness / correctness defect; **no consensus or safety impact**, incl. under a malicious leader) |
| **Class** | Broken invariant — `binary_search` over a slice that is not sorted with respect to the searcher's comparator |
| **Status** | Confirmed reachable in production via the real loader path; reproduced with unit tests |
| **Discovery** | `program-runtime/fuzz` ziggy harness (`fuzz_targets/program_cache.rs`) |

---

## Summary

`ProgramCache::assign_program` keeps each program's version list (`slot_versions`) sorted by the key
`(effective_slot, deployment_slot, account_owner, is_current_env(penv, entry))` and locates the
insertion/replacement point with `binary_search_by`. The final tiebreaker, `is_current_env`, is
evaluated **relative to the environment of whoever is currently inserting** (`penv`). As a result the
list's sort order is *observer-relative*: it is only correctly sorted for the environment that last
inserted into a given `(effective, deployment, owner)` group.

When two environments legitimately coexist at the same `(effective, deployment, owner)` — which
happens during the **epoch-boundary cache-preparation window** (`Bank` recompiles loaded programs with
the *upcoming* environment, `runtime/src/bank.rs:1599`) — a subsequent `assign_program` performed with
the *other* environment runs `binary_search` over a slice that is not sorted from its point of view.
`binary_search` on an unsorted slice returns an unspecified result; here it **fails to find the entry
it should replace** and instead:

- **(Err path)** inserts a **duplicate** entry for the same program version, or
- **(Ok path, on a later cycle)** matches a disallowed transition and takes the
  `debug_assert!(false, "Unexpected replacement of an entry")` arm
  (`program-runtime/src/loaded_programs.rs:448`).

---

## Root cause

`assign_program` (`program-runtime/src/loaded_programs.rs:411-429`):

```rust
let insertion_point = slot_versions.binary_search_by(|at| {
    at.effective_slot
        .cmp(&entry.effective_slot)
        .then(at.deployment_slot.cmp(&entry.deployment_slot))
        .then(at.account_owner.cmp(&entry.account_owner))
        .then(
            // "no effect during normal operation. Only during the cache
            //  preparation phase this does allow entries which only differ in
            //  their environment to be interleaved in slot_versions."
            is_current_env(program_runtime_environment, at.program.get_environment())
                .cmp(&is_current_env(program_runtime_environment, entry.program.get_environment())),
        )
});
```

`is_current_env(penv, x)` returns `true` iff `x` has no environment or `x`'s environment `== penv`.
Because the comparator's 4th term depends on `penv`, the relative order of two same-`(eff,dep,owner)`
entries **flips** depending on which environment is searching:

- Insert `A` (env `E_a`), then `B` (env `E_b`, `penv=E_b`): list ends up `[A, B]` — ascending **relative to `E_b`**
  (`is_current_env(E_b, A)=false=0`, `is_current_env(E_b, B)=true=1`).
- Later search with `penv=E_a`: relative to `E_a` the same list is `[true, false] = [1, 0]` — **descending**.
  `binary_search` is now operating on an unsorted slice.

The code comment asserts the tiebreaker "has no effect during normal operation," which is true, but it
does have an effect precisely in the cache-prep window the feature exists for — and that effect is a
broken `binary_search` precondition.

---

## Reproduction

Two unit tests in `program-runtime/fuzz/fuzz_targets/program_cache.rs`
(run with `cargo test --bin program_cache`):

### 1. `repro_production_path_duplicate` — the production call pattern

Uses only the real `ProgramCache` public API and two real environments:

```
1. assign_program(E_exec, key, Loaded(E_exec) @ (dep=10, eff=11))   // normal load
2. assign_program(E_up,   key, Loaded(E_up)   @ (dep=10, eff=11))   // cache-prep recompile (bank.rs:1599)
3. sort_and_unload(..)                                              // eviction: Loaded -> Unloaded
4. assign_program(E_exec, key, Loaded(E_exec) @ (dep=10, eff=11))   // reload (finish_cooperative_loading_task)
```

Resulting `slot_versions` (FAILS the "exactly one exec entry" assertion):

```
[0] Unloaded  env=exec       <- should have been replaced by step 4
[1] Loaded    env=upcoming
[2] Loaded    env=exec       <- step 4 inserted a duplicate instead of replacing [0]
```

The cache-hit short-circuit does **not** prevent step 4: an `Unloaded` entry is a cache *miss*, so the
reload genuinely fires and mis-searches.

### 2. `repro_assign_replacement` — the `debug_assert` arm

A second sequence drives a later cycle into the `_ =>` "Unexpected replacement of an entry" arm
(`loaded_programs.rs:448`), i.e. the hard panic in debug builds.

---

## Reachability (production)

Every step above corresponds to a real production action:

| Step | Production analogue | Code |
|---|---|---|
| 1 | Normal program load | `finish_cooperative_loading_task` → `assign_program` |
| 2 | Cache-prep recompile with upcoming env | `runtime/src/bank.rs:1599` |
| 3 | Eviction demotes `Loaded` → `Unloaded` | `sort_and_unload` / `evict_using_random_selection` |
| 4 | Reload after a cache miss | `svm/src/program_loader.rs:103` + `finish_cooperative_loading_task` |

**Preconditions** (narrow but real):
- An epoch boundary at which a feature activation changes the program-runtime environment (so the
  cache-prep recompile runs and creates the two-environment coexistence). Occasional on mainnet.
- Eviction demoting the execution-env entry during that window (routine, probabilistic).
- A transaction referencing the program during the window (forces the reload).

A malicious leader can make the trigger deterministic for replay nodes (deploy a program, invoke it,
pressure eviction, re-invoke) — but see Impact: this does not yield any committed-result divergence.

---

## Impact

### Consensus / safety: **none** (verified, including malicious-leader)

The two effects are node-local and non-deterministic; for either to fork consensus it must change a
*committed* result on the replay path. It cannot, for four independent reasons:

1. **Disabled on the commit/replay path.** The result-visible effect (`assign_program → true` →
   `hit_max_limit` → `Err(ProgramCacheHitMaxLimit)`) is gated behind `&& limit_to_load_programs`
   (`svm/src/transaction_processor.rs:876`), which is **`false`** on the commit path
   (`runtime/src/bank.rs:4482`, `do_load_execute_and_commit_transactions`, used by replay at
   `ledger/src/blockstore_processor.rs:254`). It is `true` only for `simulate_transaction` (RPC,
   `bank.rs:3743`) and banking-stage (`core/src/banking_stage/consumer.rs:328`).
   Note: despite the name, `limit_to_load_programs` is **not a program-count cap** — it is an
   abort-on-anomaly switch. The only thing that sets `hit_max_limit` is `assign_program` returning
   `true`, which happens *only* in the `_ =>` "Unexpected replacement of an entry" arm (`:450`); every
   normal insert/allowed-replace returns `false` (`:477`). The comment at `transaction_processor.rs:878`
   confirms: "This branch is taken when there is an error in assigning a program to a cache slot." So
   this bug is a concrete, reachable trigger of the very anomaly `hit_max_limit` exists to absorb — and
   replay disables the switch *by design* (it must execute committed txs deterministically; a node-local
   abort limit would itself be a fork vector). Replay's real bound on load work is the block's cost
   limits, not this flag.
2. **Never committed even on the leader.** `ProgramCacheHitMaxLimit` is a *not-included* reason
   (`scheduling-utils/src/error.rs:87`); such transactions are filtered out of the block
   (`consumer.rs:356`, `was_processed()`) and never reach the bank hash (`account_saver.rs:68`).
3. **Cache is semantically transparent.** Every duplicate shares `(effective, deployment, owner)` →
   same on-chain program version, differing only in environment; `extract` hard-filters by execution
   environment (`loaded_programs.rs:629`), so it only ever returns the matching-env entry. The worst a
   duplicate does is turn a hit into a miss, which reloads deterministically from bank state. Tombstones
   (`effective==deployment`) can never collide with `Loaded` (`effective==deployment+1`).
4. **Committed cost is cache-independent.** `loaded_accounts_data_size_cost`
   (`cost-model/src/cost_tracker.rs`) derives from declared account sizes, not cache hit/miss — it must
   be, since the cache is already non-deterministic across nodes.

A leader controls block contents but cannot embed cache state into a block; release validators do not
panic (debug-assertions off).

### Liveness / operational: low

- Redundant program reloads / recompiles (CPU).
- Transient duplicate `slot_versions` entries (bounded; cleared by prune/eviction at reroot).
- Leader-local: a banking batch may abort with `ProgramCacheHitMaxLimit` (retryable; within leader power).
- **Debug builds: hard crash** via the `debug_assert!` — an operator running a debug/test validator can
  crash at such an epoch boundary. This is the most likely real-world manifestation.
- RPC `simulate_transaction` may spuriously return `ProgramCacheHitMaxLimit`, degrading fee estimation.

### Fragility worth flagging

The entire no-consensus-impact argument rests on `limit_to_load_programs == false` on the commit/replay
path. **If that ever flips `true` for replay/commit, this same bug becomes a non-deterministic
consensus-liveness fork vector** (replay nodes committing divergent `ProgramCacheHitMaxLimit` results).

---

## Suggested fix

Make `slot_versions` ordering **environment-independent** so `binary_search` never operates on a slice
that is unsorted from the searcher's viewpoint. Options:

1. Replace the `is_current_env(penv, …)` tiebreaker with a **stable, penv-independent** discriminator
   (e.g. a monotonic environment id / generation counter), so the list has a single total order
   regardless of who inserts.
2. When more than one environment can coexist at a key, **linear-scan** for the exact
   `(effective, deployment, owner, env)` match instead of `binary_search`.

Either removes the observer-relative ordering. Add a regression test mirroring
`repro_production_path_duplicate`. Optionally, harden the call site by asserting
`limit_to_load_programs == false` on the commit/replay path to make the fragility explicit.

---

## Appendix: harness

The defect was surfaced by the model-based ziggy fuzzer at `program-runtime/fuzz/`
(target `program_cache`), which applies an `arbitrary`-generated op sequence to a real
`ProgramCache<MockForkGraph>` and checks structural invariants plus a property-based `extract` oracle
after every op. Mocks are exposed from the crate via the `fuzz` feature (`program-runtime/src/fuzz_util.rs`).
`AFL_DEBUG=1` prints the decoded fork topology + ops and traces each op as it executes.
