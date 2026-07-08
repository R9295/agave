# solana-svm-fuzz — instruction-execution fuzzing (ziggy)

A [ziggy](https://github.com/srlabs/ziggy)-driven fuzz harness for agave's
**single-instruction execution** path — the same code behind the conformance
C ABI entry point `sol_compat_instr_execute_v1`.

The target decodes a protobuf `InstrContext` (`org.solana.sealevel.v1`, from the
[`protosol`](https://crates.io/crates/protosol) crate), executes the instruction
against the **local** `solana-svm` `conformance` harness via
`execute_instr_proto`, and lets ziggy's coverage-guided engines (AFL++ /
honggfuzz) explore the runtime.

Two deliberate choices, per the brief:

- It uses the **local agave conformance harness** (`svm/src/conformance/`), not
  the external `solfuzz-agave` project. `solana-svm` is pulled in by path with
  its `conformance` feature enabled, so the fuzzer exercises the same
  `execute_instr_proto` / `sol_compat_instr_execute_v1` code that ships in this
  repo.
- The fuzzing shim is **ziggy**, not cargo-fuzz and not afl.rs. The harness body
  is just `ziggy::fuzz!`, and everything runs through `cargo ziggy`.

## Layout

```
svm/fuzz/
├── Cargo.toml                        # detached crate (own workspace + lockfile)
├── fuzz_targets/instr_execute.rs     # decode protobuf InstrContext -> execute
├── fuzz_targets/sanitize_differential.rs  # VIEW vs LEGACY sanitizer diff
├── fuzz_targets/il_mutate.rs         # mutate a fuzz-il fixture -> execute
└── examples/gen_corpus.rs            # writes valid protobuf seeds + self-tests
```

The crate carries its own `[workspace]` table, so it has an independent lockfile
and target dir and never affects the validator build or CI.

## Prerequisites

```sh
cargo install ziggy cargo-afl honggfuzz
```

ziggy orchestrates AFL++ and honggfuzz; `cargo-afl` builds the LLVM
instrumentation. See the [ziggy book](https://srlabs.github.io/ziggy/) for
platform setup (on Linux you may need `AFL_*` env or `cargo afl config --build`).

## Seed the corpus

Random bytes almost never decode into an executable `InstrContext`, so start
from valid seeds:

```sh
cd svm/fuzz
cargo run --release --example gen_corpus -- corpus/instr_execute
```

This also self-tests the pipeline — each seed is run through
`execute_instr_proto` and its effect printed:

```
seed   system_transfer_1000: result=0 cu_avail=199850 modified_accounts=2
  -> wrote corpus/instr_execute/system_transfer_1000.bin (NN bytes)
```

To fuzz BPF programs, drop real fixtures (e.g. from the
[`solana-conformance`](https://github.com/firedancer-io/solana-conformance) test
vectors, which are exactly these `InstrContext` protobufs) into the same corpus
dir.

## Fuzz

```sh
cargo ziggy fuzz                                  # build + run the campaign
cargo ziggy run ./output/.../crashes/<id>          # replay a crashing input
cargo ziggy cover                                  # coverage report
cargo ziggy minimize                               # shrink the corpus
```

Crashes land under `output/<target>/`. Replay with `cargo ziggy run` to get the
panic backtrace (the release profile keeps debug symbols).

## What counts as a finding

The target decodes the protobuf, then applies a precondition filter
(`preconditions_hold`) before executing. The filter mirrors the deterministic
`expect`/`assert`/`unwrap` sites in the conformance glue (proto → `InstrContext`
conversion, sysvar cache, program cache), each annotated in the source with the
exact line it guards. Inputs that would hit one of those are *malformed
fixtures*, not VM bugs, so they are skipped — keeping the campaign close to
false-positive free.

Anything that panics, aborts, or trips UB **after** the filter is a genuine
crash in agave instruction execution. There is intentionally no `catch_unwind`.

### Feature set

The harness **hardcodes every feature ON** (it overwrites `InstrContext.features`
with the full `agave_feature_set::FEATURE_NAMES` set before executing), so every
run targets the *latest* runtime. This is deliberate: an input with `features`
unset or empty would otherwise execute with all features **disabled** — the
genesis config, which is not how a current validator behaves. The trade-off is
that the `features` field is not fuzzed. To explore feature-gating instead,
delete the override block in `fuzz_targets/instr_execute.rs`.

## The `il_mutate` target

`instr_execute` decodes the fuzzer bytes *directly* as a protobuf
`InstrContext`, so most random inputs never form a valid instruction. The
`il_mutate` target attacks the same `execute_instr_proto` path from the other
end: it treats the fuzzer input as a **recipe** over a pool of hand-written,
already-valid fixtures written in [fuzz-il](../../fuzz-il).

Each iteration:

1. picks one `.il` testcase from `$CORPUS_PATH` (files listed once, sorted, then
   indexed by the first input byte),
2. lowers it with `fuzz_il::lower_source`,
3. applies **one** mutation — the second input byte's low bit selects
   `flip_invocation_is_signer` vs `flip_invocation_is_writable`; the remaining
   bytes hash into which account meta to flip. The flip lands on any call's
   account meta — system-program CPIs and direct-manipulation calls alike,
4. compiles + realizes the mutated program into an `InstrContext`
   (`fuzz_il::instr_context_from_lowered`), forces all features ON (as above),
   and runs it through `execute_instr_proto`.

This keeps every execution anchored to a semantically valid instruction while
the fuzzer explores the signer/writable privilege space around it — the kind of
divergence raw-protobuf fuzzing rarely reaches. Unparseable corpus files and
compile failures are skipped, not flagged; only a post-execution panic is a
finding.

```sh
cd svm/fuzz
# CORPUS_PATH is the IL *source* pool (mutation seeds), distinct from ziggy's
# own byte corpus under ./output.
CORPUS_PATH=../../fuzz-il/testcases cargo ziggy build --target il_mutate
CORPUS_PATH=../../fuzz-il/testcases cargo ziggy fuzz  --target il_mutate
```

> Note: this target compiles the mutated program to an SBF ELF (one clang/lld
> invocation) per iteration, so it trades throughput for valid fixtures.
> `clang-20` / `ld.lld` from the Solana LLVM toolchain must be on the toolchain
> path fuzz-il's `compiler` module expects.

## Widening the search

- **Arithmetic / invariant bugs:** uncomment `debug-assertions` and
  `overflow-checks` in `Cargo.toml`. More findings, but some are debug-only and
  not reachable in a release validator — triage accordingly.
- **Arbitrary program ids:** relax `program_is_in_cache` in
  `fuzz_targets/instr_execute.rs`. You will then see `program not loaded in
  cache` panics from the harness on most random inputs — useful only if you also
  feed a corpus where the program account is always present and loadable.
- **More builtins / loaders:** extend the `builtins` / `bpf_owners` lists in the
  same file.
