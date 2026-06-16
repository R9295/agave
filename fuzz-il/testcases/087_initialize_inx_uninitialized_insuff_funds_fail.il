# source: system_instruction.rs initialize_inx_uninitialized_acc_insuff_funds_fail
# nonce account is uninitialized but below rent-exempt balance
LoadAddress authority = account:0
InitializeNonceAccount | authority ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false)
