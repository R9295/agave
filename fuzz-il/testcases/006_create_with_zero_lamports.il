# source: test_create_with_zero_lamports
# funder may be non-system-owned because no lamports are debited
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 ForeignData 100 zeros:0 harness
LoadAccountState account:2 SystemEmpty
LoadU64 lamports = 0
LoadU64 space = 2
LoadAddress owner = program
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
