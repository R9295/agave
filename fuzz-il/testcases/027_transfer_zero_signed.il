# source: test_transfer_lamports
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | system | 100
LoadAccountState account:2 | zeros:0 | harness | 1
LoadU64 lamports = 0
Transfer | lamports ;
  (account:1, true, true),
  (account:2, true, false)
