# source: test_create_negative_lamports
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 100
LoadAccountState account:2 | zeros:0 | system | 0
LoadU64 lamports = 150
LoadU64 space = 2
LoadAddress owner = program
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
