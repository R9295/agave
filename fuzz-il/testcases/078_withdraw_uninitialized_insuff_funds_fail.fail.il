# source: system_instruction.rs withdraw_inx_unintialized_acc_insuff_funds_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceUninitialized
LoadAccountState account:2 SystemEmpty
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 18446744073709551615
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:1, false, true)
