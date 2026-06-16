# source: test_transfer_lamports
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 ForeignData 1 zeros:0 harness
LoadU64 lamports = 0
Transfer | lamports ;
  (account:1, true, false),
  (account:2, true, false)
