# source: programs/system/benches/system.rs prep_allocate_with_seed
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemEmpty
LoadAccountState account:2 SystemFunded 1000000000
LoadAddress base = account:2
LoadString seed = "shiny pepper"
LoadU64 space = 2
LoadAddress owner = system
AllocateWithSeed | base, seed, space, owner ;
  (account:1, true, false),
  (account:2, false, true)
