# source: programs/system/benches/system.rs prep_assign_with_seed
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemEmpty
LoadAccountState account:2 SystemFunded 1000000000
LoadAddress base = account:2
LoadString seed = "shiny pepper"
LoadAddress owner = system
AssignWithSeed | base, seed, owner ;
  (account:1, true, false),
  (account:2, false, true)
