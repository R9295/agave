# source: programs/system/benches/system.rs prep_assign_with_seed
LoadAccountState account:0 SystemEmpty
LoadAccountState account:1 SystemFunded 1000000000
LoadAddress base = account:1
LoadString seed = "shiny pepper"
LoadAddress owner = system
AssignWithSeed | base, seed, owner ;
  (account:0, true, false),
  (account:1, false, true)
