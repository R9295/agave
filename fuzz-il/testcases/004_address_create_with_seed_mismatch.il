# source: test_address_create_with_seed_mismatch
# harness should provide account:1 that is not create_with_seed(account:0, "dull boy", harness)
LoadAddress base = account:0
LoadString seed = "dull boy"
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = harness
CreateAccountWithSeed | base, seed, lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, false)
