# source: test_create_account_with_seed
# accounts: 0=funder/base signer+writable, 1=derived writable from seed and owner
LoadAddress base = account:0
LoadString seed = "shiny pepper"
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccountWithSeed | base, seed, lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, false)
