# source: test_create_account_with_seed_separate_base_account
# accounts: 0=funder signer+writable, 1=derived writable, 2=base signer
LoadAddress base = account:2
LoadString seed = "shiny pepper"
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccountWithSeed | base, seed, lamports, space, owner ; (account:0, true, true), (account:1, true, false), (account:2, false, true)
