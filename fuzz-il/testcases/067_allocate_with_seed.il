# source: programs/system/benches/system.rs prep_allocate_with_seed
LoadAddress base = account:1
LoadString seed = "shiny pepper"
LoadU64 space = 2
LoadAddress owner = system
AllocateWithSeed | base, seed, space, owner ; (account:0, true, false), (account:1, false, true)
