# source: test_create_account_with_seed_missing_sig
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 100
LoadAccountState 0xb5a5e39dcc1d52bb9bd401bfba993a9d6372912ce4ba05f741405159caea9e72 | zeros:0 | system | 0
LoadAddress base = account:1
LoadString seed = "shiny pepper"
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = program
CreateAccountWithSeed | base, seed, lamports, space, owner ;
  (account:1, true, false),
  (0xb5a5e39dcc1d52bb9bd401bfba993a9d6372912ce4ba05f741405159caea9e72, false, false)
