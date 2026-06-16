# source: test_create_account_allow_prefund_already_in_use
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemAllocated hex:ff
LoadAccountState account:2 SystemFunded 100
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = program
CreateAccountAllowPrefund | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
