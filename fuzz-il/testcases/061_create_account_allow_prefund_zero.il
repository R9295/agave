# source: test_create_account_allow_prefund
# account:0 is already prefunded; no payer account required by the instruction
LoadAccountState account:0 SystemFunded 100
LoadU64 lamports = 0
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccountAllowPrefund | lamports, space, owner ;
  (account:0, true, true)
