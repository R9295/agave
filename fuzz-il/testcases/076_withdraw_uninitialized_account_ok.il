# source: system_instruction.rs withdraw_inx_unintialized_acc_ok
# nonce account is uninitialized; account:2 has the nonce account key and signs
LoadAccountState account:0 NonceUninitialized
LoadAccountState account:1 SystemEmpty
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 1000042
WithdrawNonceAccount | lamports ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
