# source: system_instruction.rs withdraw_inx_initialized_acc_two_withdraws_ok
# nonce account starts initialized, partial withdraw preserves state, final withdraw deinitializes
LoadAccountState account:0 NonceInitialized account:2 1000042
LoadAccountState account:1 SystemEmpty
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 partial = 42
WithdrawNonceAccount | partial ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
LoadU64 remaining = 1000000
WithdrawNonceAccount | remaining ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
