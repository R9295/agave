# source: test_process_withdraw_ix_no_acc_data_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 | zeros:0 | system | 100
LoadAccountState account:2 | zeros:0 | system | 0
LoadAccountState account:3 | zeros:0 | system | 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
