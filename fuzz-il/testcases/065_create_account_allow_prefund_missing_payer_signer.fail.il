# source: test_create_account_allow_prefund_missing_signer
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 0
LoadAccountState account:2 | zeros:0 | system | 100
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = program
CreateAccountAllowPrefund | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, false)
