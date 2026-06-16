# source: test_assign_native_loader_and_transfer, size=0
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 ForeignData 100 zeros:0 NativeLoader1111111111111111111111111111111
LoadU64 lamports = 50
Transfer | lamports ;
  (account:1, true, true),
  (account:2, true, false)
