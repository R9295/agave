# source: test_create_account_allow_prefund
# account:1 is already prefunded; no payer account required by the instruction
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 100
LoadU64 lamports = 0
LoadU64 space = 2
LoadAddress owner = program
CreateAccountAllowPrefund | lamports, space, owner ;
  (account:1, true, true)
