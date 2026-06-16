# source: system_instruction.rs initialize_inx_initialized_account_fail
# nonce account is already initialized
LoadAddress authority = account:0
InitializeNonceAccount | authority ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false)
