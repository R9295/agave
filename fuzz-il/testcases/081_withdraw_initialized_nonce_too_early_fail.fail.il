# source: system_instruction.rs withdraw_inx_initialized_acc_nonce_too_early_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceInitializedRecent account:3 0
LoadAccountState account:2 SystemEmpty
LoadAccountState account:3 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 1447680
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
