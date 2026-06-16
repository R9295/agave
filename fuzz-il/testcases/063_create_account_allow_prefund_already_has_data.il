# source: test_create_account_allow_prefund_already_in_use
LoadAccountState account:0 SystemAllocated hex:ff
LoadAccountState account:1 SystemFunded 100
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccountAllowPrefund | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
