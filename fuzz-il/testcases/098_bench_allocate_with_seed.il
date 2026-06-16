# source: programs/system/benches/system.rs bench_allocate_with_seed
LoadAddress base = account:1
LoadString seed = "bench test"
LoadU64 space = 2
LoadAddress owner = system
AllocateWithSeed | base, seed, space, owner ; (account:0, true, false), (account:1, false, true)
