# source: system_instruction.rs withdraw_inx_initialized_acc_insuff_funds_fail
# initialized nonce account has one lamport less than requested
LoadU64 lamports = 1000043
WithdrawNonceAccount | lamports ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
