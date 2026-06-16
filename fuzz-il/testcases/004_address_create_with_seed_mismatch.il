# source: test_address_create_with_seed_mismatch
# harness should provide account:2 that is not create_with_seed(account:1, "dull boy", harness)
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemEmpty
LoadAddress base = account:1
LoadString seed = "dull boy"
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = harness
CreateAccountWithSeed | base, seed, lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, false)
