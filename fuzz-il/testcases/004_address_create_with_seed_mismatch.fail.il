# source: test_address_create_with_seed_mismatch
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemEmpty
LoadAddress base = account:1
LoadString seed = "dull boy"
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = program
CreateAccountWithSeed | base, seed, lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, false)
