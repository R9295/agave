# source: test_assign
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAddress owner = program
Assign | owner ;
  (account:1, true, false)
