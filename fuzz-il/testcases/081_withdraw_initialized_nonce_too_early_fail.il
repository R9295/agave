# source: system_instruction.rs withdraw_inx_initialized_acc_nonce_too_early_fail
# withdrawing all lamports from initialized nonce before blockhash expiry fails
LoadAccountState account:0 NonceInitialized account:2 1000042
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
