# source: system_instruction.rs withdraw_inx_unintialized_acc_insuff_funds_fail
# nonce account is uninitialized and balance is one lamport lower than requested
LoadU64 lamports = 1000043
WithdrawNonceAccount | lamports ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
