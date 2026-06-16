# source: test_create_unsigned
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemEmpty
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = program
CreateAccount | lamports, space, owner ;
  (account:1, true, false),
  (account:2, true, false)
