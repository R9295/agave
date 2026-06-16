# source: programs/system/benches/system.rs bench_create_account_with_seed
LoadAddress base = account:2
LoadString seed = "bench test"
LoadU64 lamports = 1
LoadU64 space = 2
LoadAddress owner = system
CreateAccountWithSeed | base, seed, lamports, space, owner ; (account:0, true, true), (account:1, true, false), (account:2, false, true)
