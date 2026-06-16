# source: test_process_withdraw_ix_no_acc_data_fail
LoadAccountState account:0 SystemEmpty
LoadAccountState account:1 SystemEmpty
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 1
WithdrawNonceAccount | lamports ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
