# source: test_assign
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 100
LoadAddress owner = Sysvar1111111111111111111111111111111111111
Assign | owner ;
  (account:1, true, true)
