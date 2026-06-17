# source: test_assign_native_loader_and_transfer, size=0
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 100
LoadAddress owner = NativeLoader1111111111111111111111111111111
Assign | owner ;
  (account:1, true, true)
