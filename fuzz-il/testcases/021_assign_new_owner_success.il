# source: test_assign
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
Assign | owner ;
  (account:1, true, true)
