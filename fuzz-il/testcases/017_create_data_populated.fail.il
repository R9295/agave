# source: test_create_data_populated
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemAllocated hex:00010203
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = program
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
