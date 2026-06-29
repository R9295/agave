# program-runtime fuzz

Ziggy fuzz harness for the agave **program cache** (`ProgramCache`,
`program-runtime/src/loaded_programs.rs`).

This is a **detached cargo workspace** (own `Cargo.lock`, own `target/`) so the
fuzz instrumentation never perturbs the validator build or CI — the same
isolation pattern as `svm/fuzz`. Engine is **srlabs ziggy** (AFL++ / honggfuzz),
not cargo-fuzz / afl.rs.

## Target: `program_cache`

A stateful, model-based fuzzer. `arbitrary` generates a fork topology plus a
sequence of operations (`Deploy`, `Extract`, `Prune`, `PruneByDeploymentSlot`,
`Evict`). Each op is applied to a real `ProgramCache<MockForkGraph>` and:

- **after every op** structural invariants are checked: per-key version vectors
  stay strictly sorted by `(effective_slot, deployment_slot, account_owner)`,
  and no `DelayVisibility` tombstone is ever persisted in the index;
- **every `Extract`** is checked against an independent, declarative oracle
  (`check_extract`) of what `extract` is allowed to return — wrong fork, wrong
  loader, not-yet-effective, `Unloaded`, environment-mismatch, criteria-failing
  results, and *missing an unambiguously available program* all panic, which
  ziggy records as a crash.

The mock fork graph and entry builders live in
`solana-program-runtime`'s `fuzz_util` module, exposed by its `fuzz` feature
(implied by the `solana-program-runtime = { features = ["fuzz"] }` dep here).
They mirror the private `TestForkGraphSpecific` / `new_test_entry` mocks in the
crate's test module.

### Scope / non-goals

In scope: index, fork visibility, delay visibility, eviction, pruning. Out of
scope: cooperative cross-thread loading (single-threaded here) and real
verification / JIT execution (`Loaded` entries reuse the noop ELF with neither).

`debug-assertions` are intentionally **off** (see `Cargo.toml`): the validator
runs in release, and `assign_program` / tombstone constructors contain
`debug_assert!`-guarded checks that aren't reachable in production. Fuzzing with
release semantics keeps the crash signal to our own oracle. Flip them on in the
`[profile.release]` block to widen the search.

## Running

```sh
cargo install ziggy cargo-afl honggfuzz   # one-time
cargo ziggy build
cargo ziggy fuzz -t program_cache
cargo ziggy run ./output/program_cache/crashes/<id>   # replay a divergence
```

Without ziggy installed, a plain `cargo build` produces
`target/debug/program_cache`, which replays input files passed as CLI args —
useful for reproducing a crashing input or smoke-testing a corpus.

Seed corpus: `corpus/program_cache/`.
