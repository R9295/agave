#!/usr/bin/env bash
# Build agave-validator to LLVM IR, link it into one module, dump the call
# graph, and demangle the Rust symbols in it.
set -euo pipefail

# 1. Emit LLVM IR for every crate.
rm -rf llvm-bc
mkdir -p llvm-bc
CARGO_INCREMENTAL=0 \
RUSTFLAGS="--emit=llvm-ir -C codegen-units=1 -C opt-level=0" \
cargo build -p agave-validator --bin agave-validator \
  --target x86_64-unknown-linux-gnu --target-dir target-ir
find ./target-ir/ -name "*.ll" -exec cp {} ./llvm-bc/ \;

# 2. Link into a single module.
llvm-link-23 ./llvm-bc/*.ll -o ./all.ll

# 3. Dump the call graph. The graph is printed on stderr; -disable-output
#    stops opt from also writing the module's bitcode to stdout.
opt-23 -passes=print-callgraph -disable-output ./all.ll 2> ./all.cfp

# 4. Demangle Rust symbols (cargo install rustfilt).
#    Note: demangling drops crate hashes, so distinct functions can end up
#    with the same name; the <<0x...>> node address still tells them apart.
rustfilt < ./all.cfp > ./all.cfg

# 5. Check #[source_sink::source/validate/sink] annotations against the raw
#    (mangled, so unambiguous) call graph.
cargo run -q -p source-sink --bin source-sink-check -- \
  --known-issues ./source-sink/known-issues.txt ./all.cfp
