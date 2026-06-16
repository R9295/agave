# source: test_assign_native_loader_and_transfer, size=0
LoadAccountState account:0 SystemFunded 100
LoadAccountState account:1 ForeignData 100 zeros:0 NativeLoader1111111111111111111111111111111
LoadU64 lamports = 50
Transfer | lamports ;
  (account:0, true, true),
  (account:1, true, false)
