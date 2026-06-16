# source: test_create_already_in_use
# account:2 is already owned by another program
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 ForeignEmpty 0x0505050505050505050505050505050505050505050505050505050505050505
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
