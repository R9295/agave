# source: test_transfer_lamports
# account:1 has 100 lamports; request 101
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 ForeignData 1 zeros:0 harness
LoadU64 lamports = 101
Transfer | lamports ;
  (account:1, true, true),
  (account:2, true, false)
