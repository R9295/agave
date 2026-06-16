# source: programs/system/benches/system.rs bench_transfer_with_seed
LoadString seed = "bench test"
LoadAddress owner = system
TransferWithSeed | 1, seed, owner ; (account:1, true, false), (account:2, false, true), (account:0, true, false)
