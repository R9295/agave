# source: test_assign
# assigning to current system owner needs no signature
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 100
LoadAddress owner = system
Assign | owner ;
  (account:1, true, false)
