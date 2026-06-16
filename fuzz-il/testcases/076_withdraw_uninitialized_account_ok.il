# source: system_instruction.rs withdraw_inx_unintialized_acc_ok
# nonce account is uninitialized; the nonce account key signs via a duplicate meta
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceUninitialized
LoadAccountState account:2 SystemEmpty
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 1000042
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:1, false, true)
