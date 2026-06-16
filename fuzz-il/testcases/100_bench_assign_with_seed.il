# source: programs/system/benches/system.rs bench_assign_with_seed
LoadAddress base = account:1
LoadString seed = "bench test"
LoadAddress owner = system
AssignWithSeed | base, seed, owner ; (account:0, true, false), (account:1, false, true)
