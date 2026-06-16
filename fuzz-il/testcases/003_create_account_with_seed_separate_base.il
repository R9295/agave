# source: test_create_account_with_seed_separate_base_account
# accounts: 0=funder signer+writable, 1=derived writable, 2=base signer
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemEmpty
LoadAccountState account:3 SystemFunded 100
LoadAddress base = account:3
LoadString seed = "shiny pepper"
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccountWithSeed | base, seed, lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, false),
  (account:3, false, true)
