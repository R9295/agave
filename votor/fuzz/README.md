Run from `votor/fuzz` with cargo-fuzz and a nightly Rust toolchain:

```sh
cargo fuzz run --sanitizer none votor_scenarios corpus/votor_scenarios seeds -- -dict=scenarios.dict -max_len=65536
```

Replay one input without mutations:

```sh
cargo fuzz run --sanitizer none votor_scenarios seeds/slow.json
```

Inputs use Firedancer's `test_ag_votor_scenarios` JSON action list. All ten actions
are supported: the five certificate kinds, `REPLAY_ARRIVES`, `REPLAY_COMPLETE`,
`REPLAY_DEAD`, `CLOCK` (milliseconds), and `STANDSTILL`. Labels are a slot followed
by lowercase letters (`1a`, `1b`, `2aa`); the root is `0`. Notarize and skip actions
with `part: 0`, `1`, or `2` deliver disjoint 20% stake batches. Omitting `part`
delivers all three batches, pumping the pool and voting loop between them.

The harness calls the production consensus pool and event handler synchronously,
feeds local votes back into the pool, and advances timers with a virtual clock.
It checks vote exclusivity, replay support, final-vote ancestry, standstill
rebroadcasts, monotonic roots, certificate support, canonical rooting, and the
final certified slot. Signature verification and network services are outside
this harness; direct certificates have synthetic signatures, and partial votes
are signed with deterministic validator keys. Repeated parts are deduplicated
before pool ingestion. Each input owns its temporary banks and blockstore and
starts with fresh state.

Inputs are limited to 64 slots, 256 actions, 64 KiB, and 1,000,000 ms per clock
step. Malformed inputs, inconsistent parents, replay before parent completion,
dead blocks also marked replay complete, skip parts on canonical slots,
noncanonical quorum certificates, and more than four certified blocks per slot
are rejected.
The canonical branch follows the deepest, leftmost non-skipped block back to
the root, as in Firedancer. Replay arrival is a no-op; replay dead marks a block
unavailable. Root requests are checked synchronously; asynchronous bank pruning
and snapshot handling are outside the harness.
Completed replay models block identity and parent ancestry; banks do not execute
transactions or validate block contents.

Run the seed regressions and virtual timer test from the repository root:

```sh
cargo test -p agave-votor --features agave-unstable-api event_handler::scenario_harness::tests
cargo test -p agave-votor --features agave-unstable-api test_scenario_clock
```
