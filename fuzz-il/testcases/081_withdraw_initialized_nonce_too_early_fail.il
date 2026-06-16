# source: system_instruction.rs withdraw_inx_initialized_acc_nonce_too_early_fail
# withdrawing all lamports from initialized nonce before blockhash expiry fails
LoadU64 lamports = 1000042
WithdrawNonceAccount | lamports ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
