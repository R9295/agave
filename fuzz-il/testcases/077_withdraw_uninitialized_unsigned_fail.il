# source: system_instruction.rs withdraw_inx_unintialized_acc_unsigned_fail
# nonce account is uninitialized; harness clears account:2 signer
LoadU64 lamports = 1000042
WithdrawNonceAccount | lamports ; (account:0, true, false), (account:1, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false), (account:2, false, false)
